use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};

use alpm::TransFlag;
use alpm_utils::DbListExt;
use async_trait::async_trait;
use pastor_core::{
    backend::PackageBackend,
    error::PastorError,
    package::{
        Package, PackageCategory, PackageIcon, PackageId, ParchRepoType, PackageSource,
        PackageState, PackageUpdate,
    },
    transaction::{TransactionEvent, TransactionStep},
};
use tokio::sync::mpsc::Sender;

use crate::appstream::AlpmCatalog;

#[derive(Clone)]
pub struct AlpmBackend {
    name: &'static str,
    pacman_config: pacmanconf::Config,
    catalog: Arc<RwLock<AlpmCatalog>>,
    catalog_ready: Arc<tokio::sync::Notify>,
    catalog_loaded: Arc<AtomicBool>,
    active_cancellations: Arc<RwLock<HashMap<PackageId, Arc<AtomicBool>>>>,
}

impl AlpmBackend {
    pub fn new() -> Result<Self, PastorError> {
        let pacman_config = pacmanconf::Config::new().map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Failed to load pacman.conf: {e}"),
        })?;

        // Verify that ALPM can be initialized
        let _test_alpm = Self::create_alpm(&pacman_config)?;

        let (catalog, needs_bg_load) = if let Some(cached) = AlpmCatalog::load_from_cache() {
            (Arc::new(RwLock::new(cached)), false)
        } else {
            (Arc::new(RwLock::new(AlpmCatalog::default())), true)
        };

        let catalog_ready = Arc::new(tokio::sync::Notify::new());
        let catalog_loaded = Arc::new(AtomicBool::new(!needs_bg_load));

        let backend = Self {
            name: "alpm",
            pacman_config,
            catalog: catalog.clone(),
            catalog_ready: catalog_ready.clone(),
            catalog_loaded: catalog_loaded.clone(),
            active_cancellations: Arc::new(RwLock::new(HashMap::new())),
        };

        if needs_bg_load {
            let cat_clone = catalog;
            let ready_clone = catalog_ready;
            let loaded_clone = catalog_loaded;
            tokio::task::spawn_blocking(move || {
                tracing::info!("Cold start: parsing ALPM AppStream catalog in background...");
                let cat = AlpmCatalog::load_system();
                if let Ok(mut guard) = cat_clone.write() {
                    *guard = cat;
                }
                loaded_clone.store(true, Ordering::SeqCst);
                ready_clone.notify_waiters();
                tracing::info!("Background ALPM AppStream catalog parsing complete");
            });
        }

        tracing::info!("AlpmBackend initialized successfully with native libalpm");

        Ok(backend)
    }

    pub async fn wait_catalog_ready(&self) {
        if !self.catalog_loaded.load(Ordering::SeqCst) {
            self.catalog_ready.notified().await;
        }
    }

    fn create_alpm(pacman: &pacmanconf::Config) -> Result<alpm::Alpm, PastorError> {
        let mut handle = alpm::Alpm::new(
            pacman.root_dir.as_str(),
            pacman.db_path.as_str(),
        )
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Failed to create ALPM handle: {e}"),
        })?;

        alpm_utils::configure_alpm(&mut handle, pacman).map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Failed to configure ALPM from pacman.conf: {e}"),
        })?;

        for pkg in &pacman.ignore_pkg {
            let _ = handle.add_ignorepkg(pkg.as_str());
        }
        for group in &pacman.ignore_group {
            let _ = handle.add_ignoregroup(group.as_str());
        }

        Ok(handle)
    }

    fn determine_source(alpm: &alpm::Alpm, pkg: &alpm::Package) -> PackageSource {
        let db_name = pkg.db().map(|d| d.name()).unwrap_or("local");
        if db_name == "local" {
            // Find which sync repo this package belongs to
            for sync_db in alpm.syncdbs() {
                if sync_db.pkg(pkg.name()).is_ok() {
                    let sname = sync_db.name();
                    if sname == "world" {
                        return PackageSource::Parch(ParchRepoType::World);
                    } else if sname == "void" {
                        return PackageSource::Parch(ParchRepoType::Void);
                    } else {
                        return PackageSource::Arch(sname.to_string());
                    }
                }
            }
            return PackageSource::Arch("local".to_string());
        }

        if db_name == "world" {
            PackageSource::Parch(ParchRepoType::World)
        } else if db_name == "void" {
            PackageSource::Parch(ParchRepoType::Void)
        } else {
            PackageSource::Arch(db_name.to_string())
        }
    }

    fn build_package(
        &self,
        alpm: &alpm::Alpm,
        pkg: &alpm::Package,
        installed_pkg: Option<&alpm::Package>,
        catalog: &AlpmCatalog,
    ) -> Package {
        let name = pkg.name().to_string();
        let source = Self::determine_source(alpm, pkg);
        let app_meta = catalog.get(&name);

        let groups_vec: Vec<&str> = pkg.groups().into_iter().collect();
        let (display_name, summary, description, icon, categories, screenshots) = if let Some(meta) = app_meta {
            let desc = meta.description.clone().unwrap_or_else(|| pkg.desc().unwrap_or_default().to_string());
            let sum = if !meta.summary.is_empty() {
                meta.summary.clone()
            } else {
                pkg.desc().unwrap_or_default().to_string()
            };
            (
                Some(meta.name.clone()),
                sum,
                Some(desc),
                meta.icon.clone(),
                meta.categories.clone(),
                meta.screenshots.clone(),
            )
        } else {
            let icon = PackageIcon::Themed(Self::infer_theme_icon(&name, &groups_vec));
            let categories = Self::infer_categories(&groups_vec);
            (
                None,
                pkg.desc().unwrap_or_default().to_string(),
                None,
                Some(icon),
                categories,
                vec![],
            )
        };

        let installed_version = installed_pkg.map(|p| p.version().to_string());
        let state = if let Some(ref inst_ver) = installed_version {
            if inst_ver != pkg.version().as_str() {
                PackageState::UpdateAvailable
            } else {
                PackageState::Installed
            }
        } else {
            PackageState::NotInstalled
        };

        let size_installed = if pkg.isize() > 0 {
            Some(pkg.isize() as u64)
        } else {
            None
        };

        let size_download = if pkg.download_size() > 0 {
            Some(pkg.download_size() as u64)
        } else {
            None
        };

        let dependencies: Vec<String> = pkg
            .depends()
            .iter()
            .map(|d| d.name().to_string())
            .collect();

        let license = pkg.licenses().first().map(|l| l.to_string());
        let homepage = pkg.url().map(|u| u.to_string());
        let maintainer = pkg.packager().map(|p| p.to_string());

        Package {
            id: PackageId::new(&name, source),
            name,
            display_name,
            version: pkg.version().to_string(),
            installed_version,
            summary,
            description,
            icon,
            screenshots,
            homepage,
            license,
            maintainer,
            categories,
            size_installed,
            size_download,
            dependencies,
            changelog: None,
            state,
        }
    }

    fn infer_theme_icon(name: &str, groups: &[&str]) -> String {
        for g in groups {
            match *g {
                "base-devel" => return "applications-development".to_string(),
                "multimedia" => return "applications-multimedia".to_string(),
                "games" => return "applications-games".to_string(),
                _ => {}
            }
        }
        if name.contains("terminal") || name.contains("term") || name.contains("alacritty") || name.contains("kitty") {
            "utilities-terminal".to_string()
        } else if name.contains("editor") || name.contains("code") {
            "accessories-text-editor".to_string()
        } else if name.contains("browser") || name.contains("firefox") || name.contains("chromium") {
            "internet-web-browser".to_string()
        } else {
            "package-x-generic".to_string()
        }
    }

    fn infer_categories(groups: &[&str]) -> Vec<PackageCategory> {
        let mut cats = Vec::new();
        for g in groups {
            match *g {
                "base-devel" | "development" => cats.push(PackageCategory::Development),
                "games" => cats.push(PackageCategory::Games),
                "graphics" => cats.push(PackageCategory::Graphics),
                "multimedia" => cats.push(PackageCategory::Multimedia),
                "office" => cats.push(PackageCategory::Office),
                "system" | "base" => cats.push(PackageCategory::System),
                _ => {}
            }
        }
        if cats.is_empty() {
            cats.push(PackageCategory::Utilities);
        }
        cats
    }

    fn get_cache_dir(&self) -> PathBuf {
        let candidate = self.pacman_config.cache_dir.first().map(PathBuf::from);
        candidate
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/var/cache/pacman/pkg"))
    }

    fn detect_dependencies_internal(
        pacman: &pacmanconf::Config,
        targets: &[&str],
    ) -> Result<Vec<String>, PastorError> {
        let mut alpm = Self::create_alpm(pacman)?;
        alpm.trans_init(TransFlag::NO_LOCK).map_err(|e| PastorError::TransactionFailed(format!(
            "Failed to initialize dependency resolution: {e}"
        )))?;

        let run_res: Result<Vec<String>, String> = (|| {
            for &t in targets {
                let pkg = if let Some((repo, name)) = t.split_once('/') {
                    let mut found = None;
                    for db in alpm.syncdbs() {
                        if db.name() == repo {
                            if let Ok(p) = db.pkg(name) {
                                found = Some(p);
                                break;
                            }
                        }
                    }
                    found.ok_or_else(|| format!("Package '{t}' not found in repo '{repo}'"))?
                } else {
                    alpm.syncdbs().pkg(t).map_err(|e| e.to_string())?
                };
                alpm.trans_add_pkg(pkg).map_err(|e| e.to_string())?;
            }

            alpm.trans_prepare().map_err(|e| e.to_string())?;

            let mut deps = Vec::new();
            for p in alpm.trans_add() {
                let is_target = targets.iter().any(|&t| {
                    let t_name = t.split_once('/').map(|(_, n)| n).unwrap_or(t);
                    t_name == p.name()
                });
                if !is_target {
                    deps.push(format!("{}-{}", p.name(), p.version()));
                }
            }
            Ok(deps)
        })();

        let _ = alpm.trans_release();
        run_res.map_err(|e| PastorError::TransactionFailed(format!("Dependency resolution failed: {e}")))
    }

    fn run_privileged(
        action: &str,
        extra_args: &[&str],
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        unsafe extern "C" {
            fn geteuid() -> u32;
        }

        let current_exe = std::env::current_exe().map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Could not get current executable path: {e}"),
        })?;

        let is_root = unsafe { geteuid() == 0 };
        let mut cmd = if is_root {
            let mut c = std::process::Command::new(&current_exe);
            c.arg("--alpm-worker");
            c.arg(action);
            for a in extra_args {
                c.arg(a);
            }
            c
        } else {
            let mut c = std::process::Command::new("pkexec");
            c.arg(&current_exe);
            c.arg("--alpm-worker");
            c.arg(action);
            for a in extra_args {
                c.arg(a);
            }
            c
        };

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Failed to start privileged worker: {e}"),
        })?;

        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            use std::io::BufRead;

            // ALPM download/progress callbacks emit events hundreds of times per
            // second. Forwarding every one into the (bounded) UI event channels
            // floods them; backpressure then blocks this reader, the worker's
            // stdout pipe fills up, and pacman freezes mid-download. Throttle
            // progress chatter while ALWAYS draining stdout so the privileged
            // worker is never blocked by a slow UI consumer.
            let throttle_window = std::time::Duration::from_millis(125);
            let mut last_forward = std::time::Instant::now()
                .checked_sub(throttle_window)
                .unwrap_or_else(std::time::Instant::now);

            for line in reader.lines() {
                if let Ok(line) = line {
                    if let Ok(worker_evt) = serde_json::from_str::<crate::worker::WorkerEvent>(&line) {
                        let evt = worker_evt.to_transaction_event();
                        if matches!(
                            evt.step,
                            TransactionStep::Downloading { .. }
                                | TransactionStep::ApplyingChanges { .. }
                        ) {
                            let now = std::time::Instant::now();
                            if now.duration_since(last_forward) < throttle_window {
                                continue;
                            }
                            last_forward = now;
                        }
                        let _ = progress_tx.blocking_send(evt);
                    }
                }
            }
        }

        let status = child.wait().map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Failed to wait for privileged worker: {e}"),
        })?;

        if !status.success() {
            let mut err_msg = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                let _ = stderr.read_to_string(&mut err_msg);
            }
            if err_msg.trim().is_empty() {
                err_msg = format!("Worker failed with exit code: {:?}", status.code());
            }
            return Err(PastorError::TransactionFailed(err_msg.trim().to_string()));
        }

        Ok(())
    }

    pub fn get_installed_pkg_version(&self, name: &str) -> Option<String> {
        let pacman = self.pacman_config.clone();
        if let Ok(alpm) = Self::create_alpm(&pacman) {
            alpm.localdb().pkg(name).ok().map(|p| p.version().to_string())
        } else {
            None
        }
    }

    pub fn get_foreign_packages(&self) -> Vec<(String, String)> {
        let pacman = self.pacman_config.clone();
        if let Ok(alpm) = Self::create_alpm(&pacman) {
            let syncdbs = alpm.syncdbs();
            alpm.localdb()
                .pkgs()
                .into_iter()
                .filter(|pkg| syncdbs.pkg(pkg.name()).is_err())
                .map(|pkg| (pkg.name().to_string(), pkg.version().to_string()))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn is_package_installed(&self, name: &str) -> bool {
        self.get_installed_pkg_version(name).is_some()
    }

    pub async fn install_local_package(
        &self,
        pkg_path: &std::path::Path,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let path_str = pkg_path.to_string_lossy().to_string();
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("install-file", &[&path_str], progress_tx)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Install local package task error: {e}"),
        })?
    }

    pub async fn remove_package_by_name(
        &self,
        pkg_name: &str,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let name_str = pkg_name.to_string();
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("remove", &[&name_str], progress_tx)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Remove package task error: {e}"),
        })?
    }
}

#[async_trait]
impl PackageBackend for AlpmBackend {
    fn name(&self) -> &'static str {
        self.name
    }

    fn handles(&self, id: &PackageId) -> bool {
        matches!(id.source, PackageSource::Parch(_) | PackageSource::Arch(_))
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>, PastorError> {
        let q = query.trim().to_lowercase();
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();
        let b = self.clone();

        tokio::task::spawn_blocking(move || {
            let alpm = Self::create_alpm(&pacman)?;
            let localdb = alpm.localdb();
            let catalog = catalog_arc.read().unwrap();

            let mut matched_pkgs: Vec<(i32, Package)> = Vec::new();
            let mut seen_names = std::collections::HashSet::new();

            let tokens: Vec<&str> = q.split_whitespace().collect();

            if tokens.is_empty() {
                // Return popular desktop applications when query is empty
                for (pkgname, meta) in &catalog.by_pkgname {
                    if let Some(pkg) = alpm.syncdbs().pkg(pkgname.as_str()).ok().or_else(|| localdb.pkg(pkgname.as_str()).ok()) {
                        if seen_names.insert(pkg.name().to_string()) {
                            let installed_pkg = localdb.pkg(pkg.name()).ok();
                            let p = b.build_package(&alpm, &pkg, installed_pkg, &catalog);
                            let score = if !meta.screenshots.is_empty() { 50 } else { 10 };
                            matched_pkgs.push((score, p));
                            if matched_pkgs.len() >= 60 {
                                break;
                            }
                        }
                    }
                }
            } else {
                let q_norm: String = q.chars().filter(|c| c.is_alphanumeric()).collect();
                let score_and_build = |pkg: &alpm::Package| -> (i32, Package) {
                    let name = pkg.name();
                    let name_lower = name.to_lowercase();
                    let desc_lower = pkg.desc().unwrap_or_default().to_lowercase();

                    let meta = catalog.get(name);
                    let title_lower = meta.as_ref().map(|m| m.name.to_lowercase()).unwrap_or_default();
                    let summary_lower = meta.as_ref().map(|m| m.summary.to_lowercase()).unwrap_or_default();
                    let id_lower = meta.as_ref().map(|m| m.id.to_lowercase()).unwrap_or_default();
                    let empty_kw = Vec::new();
                    let keywords = meta.as_ref().map(|m| &m.keywords).unwrap_or(&empty_kw);

                    let mut score = 0;

                    // Exact query matches
                    if name_lower == q {
                        score += 2500;
                    } else if title_lower == q {
                        score += 2200;
                    } else if name_lower.starts_with(&q) {
                        score += 1200;
                    } else if title_lower.starts_with(&q) {
                        score += 1100;
                    } else if name_lower.contains(&q) {
                        score += 800;
                    } else if title_lower.contains(&q) {
                        score += 750;
                    }

                    // Normalized continuous query match (e.g. "diskutility" matches "org.gnome.DiskUtility" or "gnome-disk-utility")
                    if !q_norm.is_empty() {
                        let id_norm: String = id_lower.chars().filter(|c| c.is_alphanumeric()).collect();
                        let name_norm: String = name_lower.chars().filter(|c| c.is_alphanumeric()).collect();
                        if id_norm.contains(&q_norm) {
                            score += 950;
                        }
                        if name_norm.contains(&q_norm) {
                            score += 900;
                        }
                    }

                    // Token-level matches
                    let mut matched_token_count = 0;
                    for t in &tokens {
                        let mut token_matched = false;
                        if name_lower.contains(t) {
                            score += 200;
                            token_matched = true;
                        }
                        if title_lower.contains(t) {
                            score += 220;
                            token_matched = true;
                        }
                        if id_lower.contains(t) {
                            score += 180;
                            token_matched = true;
                        }
                        if summary_lower.contains(t) {
                            score += 150;
                            token_matched = true;
                        }
                        if keywords.iter().any(|k| k.contains(t)) {
                            score += 160;
                            token_matched = true;
                        }
                        if desc_lower.contains(t) {
                            score += 100;
                            token_matched = true;
                        }
                        if token_matched {
                            matched_token_count += 1;
                        }
                    }

                    // All tokens matched bonus
                    if !tokens.is_empty() && matched_token_count == tokens.len() {
                        score += 800;
                    } else if matched_token_count > 0 {
                        score += (matched_token_count as i32) * 150;
                    }

                    // Desktop GUI application bonus
                    if meta.is_some() {
                        score += 250;
                        if !meta.as_ref().unwrap().screenshots.is_empty() {
                            score += 50;
                        }
                    }

                    // Already installed package bonus
                    let is_installed = localdb.pkg(name).is_ok();
                    if is_installed {
                        score += 50;
                    }

                    let installed_pkg = localdb.pkg(name).ok();
                    let p = b.build_package(&alpm, pkg, installed_pkg, &catalog);
                    (score, p)
                };

                // 1. Sync DBs: search matching ALL tokens (standard pacman -Ss behavior)
                for db in alpm.syncdbs() {
                    if let Ok(pkgs) = db.search(tokens.iter()) {
                        for pkg in pkgs {
                            if seen_names.insert(pkg.name().to_string()) {
                                matched_pkgs.push(score_and_build(&pkg));
                            }
                        }
                    }
                }

                // 2. If multi-token query, also search primary token so related tools with keyword matches are considered
                if tokens.len() > 1 {
                    for db in alpm.syncdbs() {
                        if let Ok(pkgs) = db.search([tokens[0]].iter()) {
                            for pkg in pkgs {
                                if matched_pkgs.len() >= 200 {
                                    break;
                                }
                                if seen_names.insert(pkg.name().to_string()) {
                                    matched_pkgs.push(score_and_build(&pkg));
                                }
                            }
                        }
                    }
                }

                // 3. AppStream Catalog: check pkgname, desktop title, id, summary, and keywords
                for (pkgname, meta) in &catalog.by_pkgname {
                    if seen_names.contains(pkgname) {
                        continue;
                    }

                    let pkg_lower = pkgname.to_lowercase();
                    let title_lower = meta.name.to_lowercase();
                    let id_lower = meta.id.to_lowercase();
                    let summary_lower = meta.summary.to_lowercase();

                    let matches_any = tokens.iter().any(|t| {
                        pkg_lower.contains(t)
                            || title_lower.contains(t)
                            || id_lower.contains(t)
                            || summary_lower.contains(t)
                            || meta.keywords.iter().any(|k| k.contains(t))
                    });

                    if matches_any {
                        if let Some(pkg) = alpm.syncdbs().pkg(pkgname.as_str()).ok().or_else(|| localdb.pkg(pkgname.as_str()).ok()) {
                            if seen_names.insert(pkg.name().to_string()) {
                                matched_pkgs.push(score_and_build(&pkg));
                            }
                        }
                    }
                }

                // 4. Local DB search
                if let Ok(local_matches) = localdb.search(tokens.iter()) {
                    for pkg in local_matches {
                        if seen_names.insert(pkg.name().to_string()) {
                            matched_pkgs.push(score_and_build(&pkg));
                        }
                    }
                }
            }

            matched_pkgs.sort_by(|a, b| b.0.cmp(&a.0));
            Ok(matched_pkgs.into_iter().map(|(_, p)| p).collect())
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Search task failed: {e}"),
        })?
    }

    async fn get_by_category(&self, category: PackageCategory) -> Result<Vec<Package>, PastorError> {
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();
        let b = self.clone();

        tokio::task::spawn_blocking(move || {
            let alpm = Self::create_alpm(&pacman)?;
            let localdb = alpm.localdb();
            let catalog = catalog_arc.read().unwrap();

            let mut packages = Vec::new();
            let mut seen = std::collections::HashSet::new();

            match category {
                PackageCategory::Featured => {
                    let featured = [
                        "firefox", "gimp", "vlc", "libreoffice-fresh", "blender",
                        "inkscape", "obs-studio", "audacity", "kdenlive", "telegram-desktop",
                        "discord", "steam", "keepassxc", "mpv", "neovim",
                    ];
                    for name in featured {
                        if let Some(pkg) = alpm.syncdbs().pkg(name).ok().or_else(|| localdb.pkg(name).ok()) {
                            if seen.insert(pkg.name().to_string()) {
                                let inst = localdb.pkg(pkg.name()).ok();
                                packages.push(b.build_package(&alpm, &pkg, inst, &catalog));
                            }
                        }
                    }
                }
                PackageCategory::ParchPicks => {
                    let picks = [
                        "parch-pacman", "mirrorman", "parch-alacritty", "paccache",
                        "fastfetch", "btop", "htop", "fish", "zsh", "micro",
                        "snapper", "btrfs-progs", "gparted",
                    ];
                    for name in picks {
                        if let Some(pkg) = alpm.syncdbs().pkg(name).ok().or_else(|| localdb.pkg(name).ok()) {
                            if seen.insert(pkg.name().to_string()) {
                                let inst = localdb.pkg(pkg.name()).ok();
                                packages.push(b.build_package(&alpm, &pkg, inst, &catalog));
                            }
                        }
                    }
                }
                _ => {
                    // Match packages from catalog matching category
                    for (pkgname, meta) in &catalog.by_pkgname {
                        if meta.categories.contains(&category) {
                            if let Some(pkg) = alpm.syncdbs().pkg(pkgname.as_str()).ok().or_else(|| localdb.pkg(pkgname.as_str()).ok()) {
                                if seen.insert(pkg.name().to_string()) {
                                    let inst = localdb.pkg(pkg.name()).ok();
                                    packages.push(b.build_package(&alpm, &pkg, inst, &catalog));
                                }
                            }
                        }
                    }
                }
            }

            packages.sort_by(|a, b| a.display_title().to_lowercase().cmp(&b.display_title().to_lowercase()));
            Ok(packages)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("get_by_category task failed: {e}"),
        })?
    }

    async fn curated_picks(&self) -> Result<Vec<Package>, PastorError> {
        // Spotlight carousel is Flatpak-only
        Ok(vec![])
    }

    async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError> {
        if !self.handles(id) {
            return Ok(None);
        }

        let name = id.name.clone();
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();
        let b = self.clone();

        tokio::task::spawn_blocking(move || {
            let alpm = Self::create_alpm(&pacman)?;
            let localdb = alpm.localdb();
            let catalog = catalog_arc.read().unwrap();

            let pkg_opt = alpm.syncdbs().pkg(name.as_str()).ok().or_else(|| localdb.pkg(name.as_str()).ok());
            if let Some(pkg) = pkg_opt {
                let inst = localdb.pkg(name.as_str()).ok();
                Ok(Some(b.build_package(&alpm, &pkg, inst, &catalog)))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("get_package task failed: {e}"),
        })?
    }

    async fn installed(&self) -> Result<Vec<Package>, PastorError> {
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();
        let b = self.clone();

        tokio::task::spawn_blocking(move || {
            let alpm = Self::create_alpm(&pacman)?;
            let localdb = alpm.localdb();
            let catalog = catalog_arc.read().unwrap();

            let mut packages = Vec::new();
            for pkg in localdb.pkgs() {
                packages.push(b.build_package(&alpm, &pkg, Some(&pkg), &catalog));
            }

            packages.sort_by(|a, b| a.display_title().to_lowercase().cmp(&b.display_title().to_lowercase()));
            Ok(packages)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("installed task failed: {e}"),
        })?
    }

    async fn updates(&self) -> Result<Vec<PackageUpdate>, PastorError> {
        let pacman = self.pacman_config.clone();

        tokio::task::spawn_blocking(move || {
            let mut alpm = Self::create_alpm(&pacman)?;

            // Initialize transaction with NO_LOCK for read-only upgrade check
            let flags = TransFlag::NO_LOCK;
            alpm.trans_init(flags).map_err(|e| PastorError::BackendError {
                backend: "alpm".into(),
                message: format!("Failed to initialize upgrade transaction: {e}"),
            })?;

            let mut updates = Vec::new();

            if alpm.sync_sysupgrade(false).is_ok() {
                {
                    let localdb = alpm.localdb();
                    for pkg in alpm.trans_add() {
                        if let Ok(local_pkg) = localdb.pkg(pkg.name()) {
                            let source = Self::determine_source(&alpm, &pkg);
                            let dl_size = if pkg.download_size() > 0 {
                                Some(pkg.download_size() as u64)
                            } else {
                                None
                            };

                            updates.push(PackageUpdate {
                                id: PackageId::new(pkg.name(), source),
                                current_version: local_pkg.version().to_string(),
                                new_version: pkg.version().to_string(),
                                download_size: dl_size,
                                changelog: None,
                            });
                        }
                    }
                }
                let _ = alpm.trans_release();
            } else {
                let _ = alpm.trans_release();
                let localdb = alpm.localdb();
                let ignores = &pacman.ignore_pkg;
                for local_pkg in localdb.pkgs() {
                    if ignores.iter().any(|ig| ig == local_pkg.name()) {
                        continue;
                    }
                    if let Ok(remote_pkg) = alpm.syncdbs().pkg(local_pkg.name()) {
                        if alpm::vercmp(remote_pkg.version().as_str(), local_pkg.version().as_str()).is_gt() {
                            let source = Self::determine_source(&alpm, &remote_pkg);
                            let dl_size = if remote_pkg.download_size() > 0 {
                                Some(remote_pkg.download_size() as u64)
                            } else {
                                None
                            };
                            updates.push(PackageUpdate {
                                id: PackageId::new(remote_pkg.name(), source),
                                current_version: local_pkg.version().to_string(),
                                new_version: remote_pkg.version().to_string(),
                                download_size: dl_size,
                                changelog: None,
                            });
                        }
                    }
                }
            }

            Ok(updates)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("updates task failed: {e}"),
        })?
    }

    async fn install(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let target_name = id.name.clone();
        let pacman = self.pacman_config.clone();

        // 1. Detect dependencies in user space (NO_LOCK)
        let deps = tokio::task::spawn_blocking({
            let pacman = pacman.clone();
            let target_name = target_name.clone();
            move || Self::detect_dependencies_internal(&pacman, &[&target_name])
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Dependency detection task join error: {e}"),
        })??;

        if !deps.is_empty() {
            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::CheckingDependencies,
                    progress_fraction: 0.1,
                    log_message: format!("Required dependencies: {}", deps.join(", ")),
                })
                .await;
        }

        // 2. Run privileged native worker
        let tx_clone = progress_tx.clone();
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("install", &[&target_name], tx_clone)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Install worker join error: {e}"),
        })?
    }

    async fn remove(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let target_name = id.name.clone();
        let tx_clone = progress_tx.clone();
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("remove", &[&target_name], tx_clone)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Remove worker join error: {e}"),
        })?
    }

    async fn update_all(
        &self,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("upgrade", &[], progress_tx)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Upgrade worker join error: {e}"),
        })?
    }

    async fn refresh_databases(
        &self,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("refresh", &[], progress_tx)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Refresh databases worker join error: {e}"),
        })?
    }

    async fn downgrade_versions(&self, id: &PackageId) -> Result<Vec<String>, PastorError> {
        let pkg_name = id.name.clone();
        let cache_dir = self.get_cache_dir();

        tokio::task::spawn_blocking(move || {
            let mut versions = Vec::new();
            if !cache_dir.is_dir() {
                return Ok(versions);
            }

            let Ok(entries) = std::fs::read_dir(&cache_dir) else {
                return Ok(versions);
            };

            for entry in entries.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                // Pattern: <name>-<version>-<arch>.pkg.tar.<ext>
                if let Some(rest) = file_name.strip_prefix(&format!("{}-", pkg_name)) {
                    if let Some(idx) = rest.rfind(".pkg.tar.") {
                        let ver_arch = &rest[..idx];
                        if let Some((ver, _arch)) = ver_arch.rsplit_once('-') {
                            if !versions.contains(&ver.to_string()) {
                                versions.push(ver.to_string());
                            }
                        }
                    }
                }
            }

            // Sort versions using alpm::Version descending
            versions.sort_by(|a, b| {
                let va = alpm::Version::new(a.as_str());
                let vb = alpm::Version::new(b.as_str());
                vb.cmp(&va)
            });

            Ok(versions)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("downgrade_versions task failed: {e}"),
        })?
    }

    async fn downgrade(
        &self,
        id: &PackageId,
        target_version: &str,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_name = id.name.clone();
        let target_ver = target_version.to_string();
        let cache_dir = self.get_cache_dir();

        let pkg_path = tokio::task::spawn_blocking(move || {
            // Find the cached package file
            let mut found_path = None;
            if cache_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                    for entry in entries.flatten() {
                        let fname = entry.file_name().to_string_lossy().to_string();
                        let prefix = format!("{}-{}-", pkg_name, target_ver);
                        if fname.starts_with(&prefix) && fname.contains(".pkg.tar.") {
                            found_path = Some(entry.path());
                            break;
                        }
                    }
                }
            }

            found_path.ok_or_else(|| PastorError::TransactionFailed(format!(
                "Cached package file for {} version {} not found in {}",
                pkg_name,
                target_ver,
                cache_dir.display()
            )))
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Downgrade search join error: {e}"),
        })??;

        let path_str = pkg_path.to_string_lossy().to_string();
        tokio::task::spawn_blocking(move || {
            Self::run_privileged("downgrade", &[&path_str], progress_tx)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Downgrade task error: {e}"),
        })?
    }

    async fn cancel(&self, id: &PackageId) -> Result<(), PastorError> {
        if let Ok(map) = self.active_cancellations.read() {
            if let Some(flag) = map.get(id) {
                flag.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}
