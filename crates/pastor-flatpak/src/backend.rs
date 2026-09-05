use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use libflatpak::{
    gio::Cancellable,
    prelude::*,
    Installation, Transaction,
};
use pastor_core::{
    backend::PackageBackend,
    error::PastorError,
    package::{
        Package, PackageCategory, PackageIcon, PackageId, PackageSource, PackageState,
        PackageUpdate,
    },
    transaction::{TransactionEvent, TransactionStep},
};
use tokio::sync::mpsc::Sender;

use crate::appstream::{AppstreamCatalog, AppstreamComponent};

pub struct FlatpakBackend {
    name: &'static str,
    user: bool,
    catalog: Arc<RwLock<AppstreamCatalog>>,
    default_remote: String,
    installed_cache: Arc<RwLock<Option<(std::time::Instant, HashMap<String, (String, String)>)>>>,
    active_cancellables: Arc<RwLock<HashMap<PackageId, Cancellable>>>,
}

impl FlatpakBackend {
    pub fn new(user: bool) -> Result<Self, PastorError> {
        let inst = Self::create_installation(user)?;

        let mut appstream_dirs: Vec<(String, PathBuf)> = Vec::new();
        let mut default_remote = "flathub".to_string();
        let mut found_flathub = false;

        if let Ok(remotes) = inst.list_remotes(Cancellable::NONE) {
            for remote in remotes {
                let is_disabled = remote.is_disabled();
                let is_noenumerate = remote.is_noenumerate();
                let url = remote.url();
                if is_disabled || is_noenumerate || url.is_none() {
                    continue;
                }
                let remote_name = remote.name().unwrap_or_default().to_string();
                if remote_name.contains("origin") {
                    continue;
                }
                if remote_name == "flathub" {
                    default_remote = "flathub".to_string();
                    found_flathub = true;
                } else if !found_flathub && default_remote != "flathub" {
                    default_remote = remote_name.clone();
                }
                if let Some(dir) = remote.appstream_dir(None).and_then(|x| x.path()) {
                    if dir.is_dir() {
                        appstream_dirs.push((remote_name, dir));
                    }
                }
            }
        }

        let mut catalog = AppstreamCatalog::default();
        for (remote_name, dir) in &appstream_dirs {
            let loaded = AppstreamCatalog::load_from_dir(dir, remote_name);
            if catalog.icons_dir.is_none() && loaded.icons_dir.is_some() {
                catalog.icons_dir = loaded.icons_dir.clone();
            }
            catalog.components.extend(loaded.components);
            catalog.aliases.extend(loaded.aliases);
        }

        tracing::info!(
            "FlatpakBackend initialized (user={}): {} catalog components loaded",
            user,
            catalog.components.len()
        );

        Ok(Self {
            name: "flatpak",
            user,
            catalog: Arc::new(RwLock::new(catalog)),
            default_remote,
            installed_cache: Arc::new(RwLock::new(None)),
            active_cancellables: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn create_installation(user: bool) -> Result<Installation, PastorError> {
        if user {
            Installation::new_user(Cancellable::NONE)
        } else {
            Installation::new_system(Cancellable::NONE)
        }
        .map_err(|e| PastorError::BackendError {
            backend: "flatpak".into(),
            message: format!("libflatpak initialization error: {e}"),
        })
    }

    async fn get_installed_map(&self) -> Result<HashMap<String, (String, String)>, PastorError> {
        if let Ok(guard) = self.installed_cache.read() {
            if let Some((instant, ref map)) = *guard {
                if instant.elapsed() < std::time::Duration::from_secs(30) {
                    return Ok(map.clone());
                }
            }
        }

        let user = self.user;
        let (map, is_err) = tokio::task::spawn_blocking(move || {
            let mut map = HashMap::new();
            if let Ok(inst) = Self::create_installation(user) {
                if let Ok(refs) = inst.list_installed_refs(Cancellable::NONE) {
                    for r in refs {
                        if let Some(name) = r.name() {
                            let ver = r
                                .appdata_version()
                                .or_else(|| r.branch())
                                .unwrap_or_else(|| "installed".into())
                                .to_string();
                            let origin = r.origin().unwrap_or_else(|| "flathub".into()).to_string();
                            map.insert(name.to_string(), (ver, origin));
                        }
                    }
                }
                (map, None)
            } else {
                (map, Some("Failed to get flatpak installation"))
            }
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "flatpak".into(),
            message: format!("Task join error: {e}"),
        })?;

        if let Some(err) = is_err {
            return Err(PastorError::BackendError {
                backend: "flatpak".into(),
                message: err.to_string(),
            });
        }

        if let Ok(mut guard) = self.installed_cache.write() {
            *guard = Some((std::time::Instant::now(), map.clone()));
        }

        Ok(map)
    }

    fn invalidate_installed_cache(&self) {
        if let Ok(mut guard) = self.installed_cache.write() {
            *guard = None;
        }
    }

    fn build_package(
        &self,
        app_id: &str,
        comp: Option<&AppstreamComponent>,
        installed_version: Option<String>,
        origin: Option<String>,
    ) -> Package {
        let mut remote = origin
            .or_else(|| comp.map(|c| c.remote_name.clone()))
            .unwrap_or_else(|| self.default_remote.clone());
        if remote.is_empty() || remote.contains("origin") {
            remote = self.default_remote.clone();
        }

        let effective_app_id = if let Some(c) = comp {
            c.flatpak_id.as_deref().unwrap_or_else(|| {
                app_id.strip_suffix(".desktop").unwrap_or(app_id)
            })
        } else {
            app_id.strip_suffix(".desktop").unwrap_or(app_id)
        };

        let (
            display_name,
            summary,
            description,
            license,
            maintainer,
            categories,
            icon,
            version,
            changelog,
            dependencies,
        ) = if let Some(c) = comp {
            let cat_icons = self.catalog.read().ok().and_then(|c| c.icons_dir.clone());
            let icon = c.package_icon(cat_icons.as_deref());
            let ver = c
                .releases
                .first()
                .map(|r| r.version.clone())
                .unwrap_or_else(|| "latest".to_string());
            let changelog = c.releases.first().and_then(|r| r.description.clone());
            let dependencies = if let Some(ref runtime) = c.bundle_runtime {
                vec![format!("Flatpak runtime ({})", runtime)]
            } else if !c.dependencies.is_empty() {
                c.dependencies.clone()
            } else {
                vec!["Flatpak runtime (org.freedesktop.Platform)".to_string()]
            };
            (
                Some(c.name.clone()),
                c.summary.clone(),
                c.description.clone(),
                c.project_license.clone(),
                c.developer_name.clone(),
                c.categories.clone(),
                icon,
                ver,
                changelog,
                dependencies,
            )
        } else {
            (
                None,
                format!("Flatpak application {}", effective_app_id),
                None,
                None,
                None,
                vec![PackageCategory::Utilities],
                Some(PackageIcon::Themed("package-x-generic".to_string())),
                "latest".to_string(),
                None,
                vec!["Flatpak runtime".to_string()],
            )
        };

        let is_installed = installed_version.is_some();
        let state = if is_installed {
            PackageState::Installed
        } else {
            PackageState::NotInstalled
        };

        Package {
            id: PackageId::new(effective_app_id, PackageSource::Flatpak { remote }),
            name: effective_app_id.to_string(),
            display_name,
            version: installed_version.clone().unwrap_or(version),
            installed_version,
            summary,
            description,
            icon,
            screenshots: comp.map(|c| c.screenshots.clone()).unwrap_or_default(),
            homepage: comp.and_then(|c| c.homepage.clone()),
            license,
            maintainer,
            categories,
            size_installed: None,
            size_download: None,
            dependencies,
            changelog,
            state,
        }
    }

    pub fn clean_runtime_name(name: &str) -> String {
        if let Some(rest) = name.strip_prefix("org.freedesktop.Platform.GL.nvidia-") {
            format!("Nvidia GL Driver ({})", rest)
        } else if name == "org.freedesktop.Platform.GL.default" {
            "Mesa OpenGL Driver".to_string()
        } else if let Some(rest) = name.strip_prefix("org.freedesktop.Platform.GL.") {
            format!("OpenGL Driver ({})", rest)
        } else if name == "org.freedesktop.Platform.VAAPI.nvidia" {
            "Nvidia VAAPI Driver".to_string()
        } else if name == "org.freedesktop.Platform.codecs-extra" {
            "Multimedia Codecs Extra".to_string()
        } else if let Some(rest) = name.strip_prefix("org.gtk.Gtk3theme.") {
            format!("{} GTK3 Theme", rest)
        } else if name == "org.gnome.Platform" {
            "GNOME Application Platform".to_string()
        } else if name == "org.gnome.Sdk" {
            "GNOME Software Development Kit".to_string()
        } else if name == "org.freedesktop.Platform" {
            "Freedesktop Platform".to_string()
        } else if name == "org.freedesktop.Sdk" {
            "Freedesktop SDK".to_string()
        } else if name == "org.kde.Platform" {
            "KDE Application Platform".to_string()
        } else if name == "org.kde.Sdk" {
            "KDE Software Development Kit".to_string()
        } else {
            name.to_string()
        }
    }
}

#[async_trait]
impl PackageBackend for FlatpakBackend {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>, PastorError> {
        let q = query.trim().to_lowercase();
        let installed_map = self.get_installed_map().await?;

        let catalog_guard = self.catalog.read().map_err(|_| PastorError::BackendError {
            backend: "flatpak".into(),
            message: "Failed to read AppStream catalog lock".to_string(),
        })?;

        let tokens: Vec<&str> = q.split_whitespace().collect();
        let mut scored_results: Vec<(i32, Package)> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (id, comp) in &catalog_guard.components {
            let id_lower = id.to_lowercase();
            let name_lower = comp.name.to_lowercase();
            let summary_lower = comp.summary.to_lowercase();

            let score = if q.is_empty() {
                1
            } else {
                let mut s = 0;
                if id_lower == q || name_lower == q {
                    s += 1000;
                } else if id_lower.starts_with(&q) || name_lower.starts_with(&q) {
                    s += 600;
                } else if id_lower.contains(&q) || name_lower.contains(&q) {
                    s += 400;
                }

                if summary_lower.contains(&q) {
                    s += 250;
                }

                if comp.keywords.iter().any(|k| k.contains(&q)) {
                    s += 300;
                }

                // Multi-token matching (e.g. "download manager")
                if !tokens.is_empty() {
                    let mut matched_tokens = 0;
                    for t in &tokens {
                        if id_lower.contains(t)
                            || name_lower.contains(t)
                            || summary_lower.contains(t)
                            || comp.keywords.iter().any(|k| k.contains(t))
                        {
                            matched_tokens += 1;
                        }
                    }
                    if matched_tokens == tokens.len() {
                        s += 200;
                    }
                    s += (matched_tokens as i32) * 50;
                }
                s
            };

            if score > 0 {
                let installed_info = installed_map.get(id);
                let installed_ver = installed_info.map(|(v, _)| v.clone());
                let origin = installed_info.map(|(_, o)| o.clone());

                let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                let dedupe_key = pkg.id.name.clone();
                if seen.insert(dedupe_key) {
                    scored_results.push((score, pkg));
                }

                if q.is_empty() && scored_results.len() >= 60 {
                    break;
                }
            }
        }

        // Include any installed flatpaks that might not be in the AppStream catalog
        for (inst_id, (inst_ver, origin)) in &installed_map {
            let clean_id = inst_id.strip_suffix(".desktop").unwrap_or(inst_id);
            if !seen.contains(inst_id) && !seen.contains(clean_id) {
                let inst_lower = inst_id.to_lowercase();
                let score = if q.is_empty() {
                    1
                } else if inst_lower == q {
                    1000
                } else if inst_lower.starts_with(&q) {
                    600
                } else if inst_lower.contains(&q) {
                    400
                } else if !tokens.is_empty() && tokens.iter().any(|t| inst_lower.contains(t)) {
                    100
                } else {
                    0
                };

                if score > 0 {
                    let pkg = self.build_package(
                        inst_id,
                        None,
                        Some(inst_ver.clone()),
                        Some(origin.clone()),
                    );
                    if seen.insert(pkg.id.name.clone()) {
                        scored_results.push((score, pkg));
                    }
                }
            }
        }

        scored_results.sort_by(|a, b| b.0.cmp(&a.0));
        let mut results: Vec<Package> = Vec::new();
        let mut final_seen = std::collections::HashSet::new();
        for (_, p) in scored_results {
            if final_seen.insert(p.id.name.clone()) {
                results.push(p);
            }
        }
        Ok(results)
    }

    async fn get_by_category(&self, category: PackageCategory) -> Result<Vec<Package>, PastorError> {
        let installed_map = self.get_installed_map().await.unwrap_or_default();

        let catalog_guard = self.catalog.read().map_err(|_| PastorError::BackendError {
            backend: "flatpak".into(),
            message: "Failed to read AppStream catalog lock".to_string(),
        })?;

        let mut results = Vec::new();
        match category {
            PackageCategory::Featured => {
                let featured_ids = [
                    "org.mozilla.firefox",
                    "org.videolan.VLC",
                    "org.gimp.GIMP",
                    "com.spotify.Client",
                    "org.blender.Blender",
                    "org.inkscape.Inkscape",
                    "org.libreoffice.LibreOffice",
                    "com.obsproject.Studio",
                    "org.kde.kdenlive",
                    "org.audacityteam.Audacity",
                    "org.telegram.desktop",
                    "com.discordapp.Discord",
                    "org.gnome.Boxes",
                    "com.valvesoftware.Steam",
                    "org.keepassxc.KeePassXC",
                    "io.github.shifteight.mousam",
                    "com.github.tchx84.Flatseal",
                    "org.gnome.Builder",
                    "org.signal.Signal",
                    "org.torproject.torbrowser-launcher",
                ];
                for id in featured_ids {
                    if let Some(comp) = catalog_guard.get_component(id) {
                        let installed_info = installed_map.get(id);
                        let installed_ver = installed_info.map(|(v, _)| v.clone());
                        let origin = installed_info.map(|(_, o)| o.clone());
                        let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                        if !results.iter().any(|p: &Package| p.id.name == pkg.id.name) {
                            results.push(pkg);
                        }
                    }
                }
                if results.len() < 8 {
                    for (id, comp) in &catalog_guard.components {
                        if !comp.screenshots.is_empty() {
                            let installed_info = installed_map.get(id);
                            let installed_ver = installed_info.map(|(v, _)| v.clone());
                            let origin = installed_info.map(|(_, o)| o.clone());
                            let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                            if !results.iter().any(|p: &Package| p.id.name == pkg.id.name) {
                                results.push(pkg);
                            }
                            if results.len() >= 16 {
                                break;
                            }
                        }
                    }
                }
            }
            PackageCategory::ParchPicks => {
                let picks_ids = [
                    "com.github.tchx84.Flatseal",
                    "com.mattjakeman.ExtensionManager",
                    "io.github.flattool.Warehouse",
                    "io.gitlab.adhami3310.Impression",
                    "org.gnome.Firmware",
                    "org.gnome.Loupe",
                    "org.gnome.Snapshot",
                    "org.gnome.Decibels",
                    "org.gnome.TextEditor",
                    "org.gnome.Calculator",
                    "org.gnome.SystemMonitor",
                    "org.gnome.FileRoller",
                    "org.gnome.baobab",
                    "org.gnome.Weather",
                    "org.gnome.Maps",
                    "org.gnome.Clocks",
                ];
                for id in picks_ids {
                    if let Some(comp) = catalog_guard.get_component(id) {
                        let installed_info = installed_map.get(id);
                        let installed_ver = installed_info.map(|(v, _)| v.clone());
                        let origin = installed_info.map(|(_, o)| o.clone());
                        let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                        if !results.iter().any(|p: &Package| p.id.name == pkg.id.name) {
                            results.push(pkg);
                        }
                    }
                }
                if results.is_empty() {
                    for (id, comp) in &catalog_guard.components {
                        if comp.categories.contains(&PackageCategory::Utilities)
                            || comp.categories.contains(&PackageCategory::System)
                        {
                            let installed_info = installed_map.get(id);
                            let installed_ver = installed_info.map(|(v, _)| v.clone());
                            let origin = installed_info.map(|(_, o)| o.clone());
                            let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                            if !results.iter().any(|p: &Package| p.id.name == pkg.id.name) {
                                results.push(pkg);
                            }
                            if results.len() >= 10 {
                                break;
                            }
                        }
                    }
                }
            }
            _ => {
                for (id, comp) in &catalog_guard.components {
                    if comp.categories.contains(&category) {
                        let installed_info = installed_map.get(id);
                        let installed_ver = installed_info.map(|(v, _)| v.clone());
                        let origin = installed_info.map(|(_, o)| o.clone());

                        let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                        if !results.iter().any(|p: &Package| p.id.name == pkg.id.name) {
                            results.push(pkg);
                        }
                    }
                }
                results.sort_by(|a, b| a.display_title().to_lowercase().cmp(&b.display_title().to_lowercase()));
            }
        }
        Ok(results)
    }

    async fn curated_picks(&self) -> Result<Vec<Package>, PastorError> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let app_ids = tokio::task::spawn_blocking(move || {
            let mut ids = Vec::new();

            // 1. App of the Day
            let url_day = format!("https://flathub.org/api/v2/app-picks/app-of-the-day/{}", today);
            if let Ok(resp) = ureq::get(&url_day).timeout(std::time::Duration::from_secs(4)).call() {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    if let Some(app_id) = json.get("app_id").and_then(|v| v.as_str()) {
                        ids.push(app_id.to_string());
                    }
                }
            }

            // 2. Apps of the Week
            let url_week = format!("https://flathub.org/api/v2/app-picks/apps-of-the-week/{}", today);
            if let Ok(resp) = ureq::get(&url_week).timeout(std::time::Duration::from_secs(4)).call() {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    if let Some(apps) = json.get("apps").and_then(|v| v.as_array()) {
                        for item in apps {
                            if let Some(app_id) = item.get("app_id").and_then(|v| v.as_str()) {
                                if !ids.contains(&app_id.to_string()) {
                                    ids.push(app_id.to_string());
                                }
                            }
                        }
                    }
                }
            }

            // 3. Curated App Selections
            let url_curated = format!("https://flathub.org/api/v2/app-picks/curated-app-selections/{}", today);
            if let Ok(resp) = ureq::get(&url_curated).timeout(std::time::Duration::from_secs(4)).call() {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    if let Some(selections) = json.get("selections").and_then(|v| v.as_array()) {
                        for sel in selections {
                            if let Some(apps) = sel.get("apps").and_then(|v| v.as_array()) {
                                for item in apps {
                                    if let Some(app_id) = item.get("app_id").and_then(|v| v.as_str()) {
                                        if !ids.contains(&app_id.to_string()) {
                                            ids.push(app_id.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            ids
        })
        .await
        .unwrap_or_default();

        let installed_map = self.get_installed_map().await.unwrap_or_default();

        let catalog_guard = self.catalog.read().map_err(|_| PastorError::BackendError {
            backend: "flatpak".into(),
            message: "Failed to read AppStream catalog lock".to_string(),
        })?;

        let mut packages = Vec::new();
        for id in &app_ids {
            if let Some(comp) = catalog_guard.get_component(id) {
                let installed_info = installed_map.get(id);
                let installed_ver = installed_info.map(|(v, _)| v.clone());
                let origin = installed_info.map(|(_, o)| o.clone());

                let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                packages.push(pkg);
            }
        }

        // If offline or flathub picks returned empty, fallback to popular apps from catalog
        if packages.is_empty() {
            let fallback_ids = [
                "org.mozilla.firefox",
                "org.videolan.VLC",
                "org.gimp.GIMP",
                "com.spotify.Client",
                "org.blender.Blender",
                "org.inkscape.Inkscape",
                "org.libreoffice.LibreOffice",
                "com.obsproject.Studio",
            ];
            for id in fallback_ids {
                if let Some(comp) = catalog_guard.get_component(id) {
                    let installed_info = installed_map.get(id);
                    let installed_ver = installed_info.map(|(v, _)| v.clone());
                    let origin = installed_info.map(|(_, o)| o.clone());

                    let pkg = self.build_package(id, Some(comp), installed_ver, origin);
                    packages.push(pkg);
                }
            }
        }

        Ok(packages)
    }

    async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError> {
        let PackageSource::Flatpak { ref remote } = id.source else {
            return Ok(None);
        };

        if id.name.starts_with("runtime/") || id.name.starts_with("app/") {
            let installed = self.installed().await.unwrap_or_default();
            if let Some(pkg) = installed.into_iter().find(|p| p.id.name == id.name) {
                return Ok(Some(pkg));
            }
        }

        let installed_map = self.get_installed_map().await.unwrap_or_default();
        let installed_info = installed_map.get(&id.name);
        let installed_ver = installed_info.map(|(v, _)| v.clone());

        let catalog_guard = self.catalog.read().map_err(|_| PastorError::BackendError {
            backend: "flatpak".into(),
            message: "Failed to read AppStream catalog lock".to_string(),
        })?;

        let comp = catalog_guard.get_component(&id.name);
        let pkg = self.build_package(&id.name, comp, installed_ver, Some(remote.clone()));
        Ok(Some(pkg))
    }

    async fn installed(&self) -> Result<Vec<Package>, PastorError> {
        let user = self.user;

        let raw_installed = tokio::task::spawn_blocking(move || {
            let inst = Self::create_installation(user)?;
            let refs = inst
                .list_installed_refs(Cancellable::NONE)
                .map_err(|e| PastorError::BackendError {
                    backend: "flatpak".into(),
                    message: format!("Failed to list installed flatpaks: {e}"),
                })?;

            let mut list = Vec::new();
            for r in refs {
                let is_app = r.kind() == libflatpak::RefKind::App;
                let is_runtime = r.kind() == libflatpak::RefKind::Runtime;
                if !is_app && !is_runtime {
                    continue;
                }
                if let Some(name) = r.name() {
                    let ver = r
                        .appdata_version()
                        .or_else(|| r.branch())
                        .unwrap_or_else(|| "installed".into())
                        .to_string();
                    let origin = r.origin().unwrap_or_else(|| "flathub".into()).to_string();
                    let branch = r.branch().unwrap_or_else(|| "stable".into()).to_string();
                    let arch = r.arch().unwrap_or_else(|| "x86_64".into()).to_string();
                    let size = r.installed_size();
                    let full_ref = r.format_ref().map(|s| s.to_string()).unwrap_or_else(|| {
                        let kind_str = if is_runtime { "runtime" } else { "app" };
                        format!("{}/{}/{}/{}", kind_str, name, arch, branch)
                    });
                    list.push((name.to_string(), ver, origin, is_runtime, branch, full_ref, size));
                }
            }
            Ok::<Vec<(String, String, String, bool, String, String, u64)>, PastorError>(list)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "flatpak".into(),
            message: format!("Task join error: {e}"),
        })??;

        let catalog_guard = self.catalog.read().map_err(|_| PastorError::BackendError {
            backend: "flatpak".into(),
            message: "Failed to read AppStream catalog lock".to_string(),
        })?;

        let mut packages = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (name, ver, origin, is_runtime, branch, full_ref, size) in raw_installed {
            if is_runtime {
                let display_title = format!("{} ({})", Self::clean_runtime_name(&name), branch);
                let pkg_id_str = full_ref.clone();
                if !seen.insert(pkg_id_str.clone()) {
                    continue;
                }

                let icon = if name.contains("Sdk") || name.contains("SDK") {
                    PackageIcon::Themed("applications-development".to_string())
                } else if name.contains("GL") || name.contains("VAAPI") {
                    PackageIcon::Themed("video-display".to_string())
                } else if name.contains("Gtk3theme") {
                    PackageIcon::Themed("preferences-desktop-theme".to_string())
                } else {
                    PackageIcon::Themed("system-run-symbolic".to_string())
                };

                let pkg = Package {
                    id: PackageId::new(&pkg_id_str, PackageSource::Flatpak { remote: origin }),
                    name: pkg_id_str,
                    display_name: Some(display_title),
                    version: branch.clone(),
                    installed_version: Some(ver),
                    summary: format!("Flatpak Runtime • {}", name),
                    description: Some(format!(
                        "Flatpak runtime environment and shared dependencies (Ref: {}).\nInstalled to provide libraries and runtime services for Flatpak applications.",
                        full_ref
                    )),
                    icon: Some(icon),
                    screenshots: vec![],
                    homepage: None,
                    license: None,
                    maintainer: Some("Flatpak".to_string()),
                    categories: vec![PackageCategory::System, PackageCategory::Development],
                    size_installed: if size > 0 { Some(size) } else { None },
                    size_download: None,
                    dependencies: vec![],
                    changelog: None,
                    state: PackageState::Installed,
                };
                packages.push(pkg);
            } else {
                let comp = catalog_guard.get_component(&name);
                let pkg = self.build_package(&name, comp, Some(ver), Some(origin));
                if seen.insert(pkg.id.name.clone()) {
                    packages.push(pkg);
                }
            }
        }

        Ok(packages)
    }

    async fn updates(&self) -> Result<Vec<PackageUpdate>, PastorError> {
        let user = self.user;

        let raw_updates = tokio::task::spawn_blocking(move || {
            let inst = Self::create_installation(user)?;
            let update_refs = inst
                .list_installed_refs_for_update(Cancellable::NONE)
                .map_err(|e| PastorError::BackendError {
                    backend: "flatpak".into(),
                    message: format!("Failed to list flatpak updates: {e}"),
                })?;

            let mut list = Vec::new();
            for r in update_refs {
                if r.kind() == libflatpak::RefKind::App {
                    if let Some(name) = r.name() {
                        let current_ver = r
                            .appdata_version()
                            .or_else(|| r.branch())
                            .unwrap_or_else(|| "unknown".into())
                            .to_string();
                        let origin = r.origin().unwrap_or_else(|| "flathub".into()).to_string();
                        list.push((name.to_string(), current_ver, origin));
                    }
                }
            }
            Ok::<Vec<(String, String, String)>, PastorError>(list)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "flatpak".into(),
            message: format!("Task join error: {e}"),
        })??;

        let catalog_guard = self.catalog.read().map_err(|_| PastorError::BackendError {
            backend: "flatpak".into(),
            message: "Failed to read AppStream catalog lock".to_string(),
        })?;

        let mut updates = Vec::new();
        for (name, current_ver, origin) in raw_updates {
            let comp = catalog_guard.get_component(&name);
            let release = comp.and_then(|c| c.releases.first());
            let new_ver = release
                .map(|r| r.version.clone())
                .unwrap_or_else(|| "latest".to_string());
            let changelog = release.and_then(|r| r.description.clone());

            updates.push(PackageUpdate {
                id: PackageId::new(&name, PackageSource::Flatpak { remote: origin }),
                current_version: current_ver,
                new_version: new_ver,
                download_size: None,
                changelog,
            });
        }

        Ok(updates)
    }

    async fn install(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let PackageSource::Flatpak { ref remote } = id.source else {
            return Err(PastorError::InvalidOperation("Invalid package source for Flatpak".into()));
        };

        let user = self.user;
        let mut remote_name = remote.clone();
        if remote_name.is_empty() || remote_name.contains("origin") {
            remote_name = self.default_remote.clone();
        }
        let raw_app_id = id.name.clone();

        // Resolve component from catalog to get exact bundle ref and flatpak id
        let (resolved_app_id, catalog_bundle_ref) = if let Ok(guard) = self.catalog.read() {
            if let Some(comp) = guard.get_component(&raw_app_id) {
                (
                    comp.flatpak_id.clone().unwrap_or_else(|| {
                        raw_app_id.strip_suffix(".desktop").unwrap_or(&raw_app_id).to_string()
                    }),
                    comp.bundle_ref.clone(),
                )
            } else {
                (raw_app_id.strip_suffix(".desktop").unwrap_or(&raw_app_id).to_string(), None)
            }
        } else {
            (raw_app_id.strip_suffix(".desktop").unwrap_or(&raw_app_id).to_string(), None)
        };

        let cancellable = Cancellable::new();
        if let Ok(mut guard) = self.active_cancellables.write() {
            guard.insert(id.clone(), cancellable.clone());
        }

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::SyncingDatabase,
                progress_fraction: 0.05,
                log_message: format!("Resolving Flatpak {} from {}...", resolved_app_id, remote_name),
            })
            .await;

        let c_for_tx = cancellable.clone();
        let res = tokio::task::spawn_blocking(move || {
            let inst = Self::create_installation(user)?;
            let tx = Transaction::for_installation(&inst, Some(&c_for_tx))
                .map_err(|e| PastorError::TransactionFailed(format!("Failed to initialize transaction: {e}")))?;

            let total_ops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));
            let completed_ops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

            let tot_ready = total_ops.clone();
            tx.connect_ready(move |t| {
                let count = t.operations().len().max(1);
                tot_ready.store(count, std::sync::atomic::Ordering::SeqCst);
                true
            });

            let comp_done = completed_ops.clone();
            tx.connect_operation_done(move |_t, _op, _commit, _res| {
                comp_done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });

            let p_tx = progress_tx.clone();
            let tot_ops = total_ops.clone();
            let comp_ops = completed_ops.clone();

            tx.connect_new_operation(move |_tx, op, progress| {
                let op_ref = op.get_ref().unwrap_or_default();
                let p_s = p_tx.clone();
                let last_pct = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(-1));
                let last_phase = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
                let last_emit = std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
                let t_ops = tot_ops.clone();
                let c_ops = comp_ops.clone();

                progress.connect_changed(move |p| {
                    let pct_raw = p.progress();
                    let cur_done = c_ops.load(std::sync::atomic::Ordering::SeqCst);
                    let total = t_ops.load(std::sync::atomic::Ordering::SeqCst).max(cur_done + 1);
                    let op_pct = (pct_raw as f32) / 100.0;
                    let aggregate_pct = ((cur_done as f32 + op_pct) / (total as f32)).min(0.99);

                    let status = p.status().unwrap_or_default().to_string();
                    let phase = if status.starts_with("Downloading metadata") {
                        "Downloading metadata"
                    } else if status.starts_with("Downloading") {
                        "Downloading"
                    } else if status.starts_with("Installing") {
                        "Installing"
                    } else if status.starts_with("Initializing") {
                        "Initializing"
                    } else if status.is_empty() {
                        "Processing"
                    } else {
                        status.split(':').next().unwrap_or(&status)
                    };

                    let mut prev_phase = last_phase.lock().unwrap();
                    let prev_pct = last_pct.load(std::sync::atomic::Ordering::Relaxed);
                    let phase_changed = *prev_phase != phase;
                    let pct_stepped = (pct_raw / 10) != (prev_pct / 10);

                    let mut emit_guard = last_emit.lock().unwrap();
                    let interval_elapsed = emit_guard.elapsed() >= std::time::Duration::from_millis(300);

                    if phase_changed || pct_stepped || pct_raw == 100 || (interval_elapsed && pct_raw != prev_pct) {
                        *prev_phase = phase.to_string();
                        last_pct.store(pct_raw, std::sync::atomic::Ordering::Relaxed);
                        *emit_guard = std::time::Instant::now();

                        let msg = if total > 1 {
                            format!("[{}/{}] {}: {} ({}%)", cur_done + 1, total, op_ref, status, pct_raw)
                        } else if status.is_empty() {
                            format!("{}: {}%", op_ref, pct_raw)
                        } else {
                            format!("{}: {} ({}%)", op_ref, status, pct_raw)
                        };
                        let _ = p_s.blocking_send(TransactionEvent {
                            step: TransactionStep::Downloading {
                                current_bytes: (aggregate_pct * 100.0) as u64,
                                total_bytes: 100,
                                speed_bps: 0,
                            },
                            progress_fraction: aggregate_pct,
                            log_message: msg,
                        });
                    }
                });
            });

            let arch = libflatpak::default_arch().unwrap_or_else(|| "x86_64".into());
            let full_ref = if let Some(b_ref) = catalog_bundle_ref {
                b_ref
            } else {
                format!("app/{}/{}/stable", resolved_app_id, arch)
            };

            if let Err(e) = tx.add_install(&remote_name, &full_ref, &[]) {
                let alt_ref = if full_ref.contains(".desktop") {
                    full_ref.replace(".desktop", "")
                } else {
                    format!("app/{}.desktop/{}/stable", resolved_app_id, arch)
                };
                if let Err(e2) = tx.add_install(&remote_name, &alt_ref, &[]) {
                    return Err(PastorError::TransactionFailed(format!(
                        "Failed to queue install for {}: {} (fallback {}: {})",
                        full_ref, e, alt_ref, e2
                    )));
                }
            }

            let run_res = tx.run(Some(&c_for_tx));

            if c_for_tx.is_cancelled() {
                let _ = progress_tx.blocking_send(TransactionEvent {
                    step: TransactionStep::Cancelled,
                    progress_fraction: 0.0,
                    log_message: format!("Installation of {} cancelled", resolved_app_id),
                });
                return Ok::<(), PastorError>(());
            }

            run_res.map_err(|e| PastorError::TransactionFailed(format!("Flatpak transaction failed: {e}")))?;

            let _ = progress_tx.blocking_send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: resolved_app_id.clone(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.99,
                log_message: "Finishing installation and configuring system triggers...".to_string(),
            });

            let _ = progress_tx.blocking_send(TransactionEvent {
                step: TransactionStep::Completed,
                progress_fraction: 1.0,
                log_message: format!("Successfully installed Flatpak {}", resolved_app_id),
            });

            Ok::<(), PastorError>(())
        })
        .await
        .map_err(|e| PastorError::TransactionFailed(format!("Join error: {e}")))?;

        if let Ok(mut guard) = self.active_cancellables.write() {
            guard.remove(id);
        }
        self.invalidate_installed_cache();
        res
    }

    async fn remove(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let user = self.user;
        let clean_id = id.name.strip_suffix(".desktop").unwrap_or(&id.name).to_string();

        let cancellable = Cancellable::new();
        if let Ok(mut guard) = self.active_cancellables.write() {
            guard.insert(id.clone(), cancellable.clone());
        }

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: clean_id.clone(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.1,
                log_message: format!("Queueing removal of Flatpak {}...", clean_id),
            })
            .await;

        let c_for_tx = cancellable.clone();
        let clean_id_clone = clean_id.clone();
        let res = tokio::task::spawn_blocking(move || {
            let inst = Self::create_installation(user)?;
            let tx = Transaction::for_installation(&inst, Some(&c_for_tx))
                .map_err(|e| PastorError::TransactionFailed(format!("Failed to initialize transaction: {e}")))?;

            let p_tx = progress_tx.clone();
            tx.connect_new_operation(move |_tx, op, progress| {
                let op_ref = op.get_ref().unwrap_or_default();
                let p_s = p_tx.clone();
                progress.connect_changed(move |p| {
                    let pct_raw = p.progress();
                    let op_pct = (pct_raw as f32) / 100.0;
                    let _ = p_s.blocking_send(TransactionEvent {
                        step: TransactionStep::ApplyingChanges {
                            package: op_ref.to_string(),
                            index: 1,
                            total: 1,
                        },
                        progress_fraction: op_pct,
                        log_message: format!("Removing {}: {}%", op_ref, pct_raw),
                    });
                });
            });

            let mut queued = false;
            // 1. If clean_id is already a formatted ref (runtime/..., app/...)
            if clean_id_clone.starts_with("runtime/") || clean_id_clone.starts_with("app/") {
                if tx.add_uninstall(&clean_id_clone).is_ok() {
                    queued = true;
                }
            }

            // 2. Query installed refs
            if !queued {
                if let Ok(refs) = inst.list_installed_refs(Cancellable::NONE) {
                    for r in refs {
                        let matches_name = r.name().map(|n| n.as_str() == clean_id_clone || n.as_str() == format!("{}.desktop", clean_id_clone)).unwrap_or(false);
                        let matches_ref = r.format_ref().map(|f| f.as_str() == clean_id_clone).unwrap_or(false);
                        if matches_name || matches_ref {
                            if let Some(f_ref) = r.format_ref() {
                                if tx.add_uninstall(f_ref.as_str()).is_ok() {
                                    queued = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // 3. Fallback standard app ref
            if !queued {
                let arch = libflatpak::default_arch().unwrap_or_else(|| "x86_64".into());
                let full_ref = format!("app/{}/{}/stable", clean_id_clone, arch);
                if let Err(e) = tx.add_uninstall(&full_ref) {
                    let alt_ref = format!("app/{}.desktop/{}/stable", clean_id_clone, arch);
                    if let Err(e2) = tx.add_uninstall(&alt_ref) {
                        return Err(PastorError::TransactionFailed(format!(
                            "Failed to queue uninstall for {}: {} ({})",
                            clean_id_clone, e, e2
                        )));
                    }
                }
            }

            let run_res = tx.run(Some(&c_for_tx));
            if c_for_tx.is_cancelled() {
                let _ = progress_tx.blocking_send(TransactionEvent {
                    step: TransactionStep::Cancelled,
                    progress_fraction: 0.0,
                    log_message: format!("Removal of Flatpak {} cancelled", clean_id_clone),
                });
                return Ok::<(), PastorError>(());
            }

            run_res.map_err(|e| PastorError::TransactionFailed(format!("Uninstall failed: {e}")))?;

            let _ = progress_tx.blocking_send(TransactionEvent {
                step: TransactionStep::Completed,
                progress_fraction: 1.0,
                log_message: format!("Successfully uninstalled Flatpak {}", clean_id_clone),
            });

            Ok::<(), PastorError>(())
        })
        .await
        .map_err(|e| PastorError::TransactionFailed(format!("Join error: {e}")))?;

        if let Ok(mut guard) = self.active_cancellables.write() {
            guard.remove(id);
        }
        self.invalidate_installed_cache();
        res
    }

    async fn cancel(&self, id: &PackageId) -> Result<(), PastorError> {
        let clean_name = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
        if let Ok(guard) = self.active_cancellables.read() {
            if let Some(c) = guard.get(id) {
                c.cancel();
                return Ok(());
            }
            for (k, c) in guard.iter() {
                let k_clean = k.name.strip_suffix(".desktop").unwrap_or(&k.name);
                if k_clean == clean_name {
                    c.cancel();
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    async fn downgrade_versions(&self, _id: &PackageId) -> Result<Vec<String>, PastorError> {
        Ok(vec![])
    }

    async fn downgrade(
        &self,
        _id: &PackageId,
        _target_version: &str,
        _progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        Err(PastorError::InvalidOperation("Downgrading is not supported for Flatpaks directly".into()))
    }
}
