use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use futures::future::join_all;
pub use pastor_core::*;
use pastor_flatpak::FlatpakBackend;
#[cfg(feature = "mock")]
use pastor_mock::MockPackageBackend;
use tokio::sync::{
    broadcast,
    mpsc::{self, Receiver, Sender},
};

#[derive(Clone, Debug)]
pub struct ActiveTransactionInfo {
    pub package_id: PackageId,
    pub package_name: String,
    pub progress_fraction: f32,
    pub step: TransactionStep,
    pub log_message: String,
}

#[derive(Clone, Debug)]
pub enum ActiveTransactionEvent {
    Started(ActiveTransactionInfo),
    Progress(ActiveTransactionInfo),
    Completed(PackageId),
    Failed(PackageId, String),
    Cancelled(PackageId),
}

#[derive(Clone)]
pub struct Store {
    backends: Vec<Arc<dyn PackageBackend>>,
    snapshot_backend: Arc<dyn SnapshotBackend>,
    modules: Arc<RwLock<ModuleConfig>>,
    active_transactions: Arc<RwLock<HashMap<PackageId, ActiveTransactionInfo>>>,
    tx_broadcaster: broadcast::Sender<ActiveTransactionEvent>,
}

impl Store {
    pub fn new() -> Self {
        let mut backends: Vec<Arc<dyn PackageBackend>> = Vec::new();
        let snapshot_backend: Arc<dyn SnapshotBackend> = Arc::new(NullSnapshotBackend);

        // Initialize real Flatpak native backend via libflatpak
        match FlatpakBackend::new(false) {
            Ok(flatpak) => {
                backends.push(Arc::new(flatpak));
                tracing::info!("Flatpak native backend registered successfully");
            }
            Err(e) => {
                tracing::warn!("Failed to initialize system Flatpak backend: {e}");
                // Fallback to user installation if system is unavailable
                if let Ok(user_flatpak) = FlatpakBackend::new(true) {
                    backends.push(Arc::new(user_flatpak));
                    tracing::info!("User Flatpak native backend registered successfully");
                }
            }
        }

        let (tx_broadcaster, _) = broadcast::channel(200);

        Self {
            backends,
            snapshot_backend,
            modules: Arc::new(RwLock::new(ModuleConfig::default())),
            active_transactions: Arc::new(RwLock::new(HashMap::new())),
            tx_broadcaster,
        }
    }

    pub fn with_backends(
        backends: Vec<Arc<dyn PackageBackend>>,
        snapshot_backend: Arc<dyn SnapshotBackend>,
    ) -> Self {
        let (tx_broadcaster, _) = broadcast::channel(200);
        Self {
            backends,
            snapshot_backend,
            modules: Arc::new(RwLock::new(ModuleConfig::default())),
            active_transactions: Arc::new(RwLock::new(HashMap::new())),
            tx_broadcaster,
        }
    }

    #[cfg(feature = "mock")]
    pub fn new_mock() -> Self {
        let mock_backend = Arc::new(MockPackageBackend::new());
        let snap = mock_backend.snapshot_backend();
        let (tx_broadcaster, _) = broadcast::channel(200);
        Self {
            backends: vec![mock_backend],
            snapshot_backend: snap,
            modules: Arc::new(RwLock::new(ModuleConfig::default())),
            active_transactions: Arc::new(RwLock::new(HashMap::new())),
            tx_broadcaster,
        }
    }

    pub fn snapshot_backend(&self) -> Arc<dyn SnapshotBackend> {
        self.snapshot_backend.clone()
    }

    pub fn module_config(&self) -> ModuleConfig {
        self.modules.read().unwrap().clone()
    }

    pub fn set_module_config(&self, config: ModuleConfig) {
        *self.modules.write().unwrap() = config;
    }

    pub async fn search_raw(&self, query: &str) -> Result<Vec<Package>, PastorError> {
        let mut futures = Vec::new();
        for b in &self.backends {
            futures.push(b.search(query));
        }

        let results = join_all(futures).await;
        let mut all_pkgs = Vec::new();
        let config = self.module_config();

        for res in results {
            match res {
                Ok(pkgs) => {
                    for p in pkgs {
                        if config.is_source_enabled(&p.id.source) {
                            all_pkgs.push(p);
                        }
                    }
                }
                Err(e) => tracing::warn!("Backend search error: {e}"),
            }
        }

        // Priority sorting:
        // 1. Installed packages come first
        // 2. Parch [world]
        // 3. Parch [void]
        // 4. Arch
        // 5. Flatpak
        // 6. AUR
        // 7. Waydroid / Bootc
        all_pkgs.sort_by(|a, b| {
            let a_inst = if a.is_installed() { 0 } else { 1 };
            let b_inst = if b.is_installed() { 0 } else { 1 };
            if a_inst != b_inst {
                return a_inst.cmp(&b_inst);
            }
            let priority = |p: &Package| match &p.id.source {
                PackageSource::Parch(ParchRepoType::World) => 0,
                PackageSource::Parch(ParchRepoType::Void) => 1,
                PackageSource::Arch(_) => 2,
                PackageSource::Flatpak { .. } => 3,
                PackageSource::Aur => 4,
                PackageSource::Bootc => 5,
                PackageSource::Waydroid => 6,
            };
            priority(a).cmp(&priority(b))
        });

        // Enrich packages lacking icons with Flathub/AppStream icons matching the same app name
        let mut icon_map: std::collections::HashMap<String, PackageIcon> = std::collections::HashMap::new();
        for p in &all_pkgs {
            if let Some(ref icon) = p.icon {
                if matches!(icon, PackageIcon::LocalPath(_)) {
                    let short = p.name.rsplit('.').next().unwrap_or(&p.name).to_lowercase();
                    icon_map.insert(short, icon.clone());
                    icon_map.insert(p.display_title().to_lowercase(), icon.clone());
                }
            }
        }

        for p in &mut all_pkgs {
            let is_generic = match &p.icon {
                None => true,
                Some(PackageIcon::Themed(name))
                    if name == "package-x-generic" || name == "application-x-executable" =>
                {
                    true
                }
                _ => false,
            };
            if is_generic {
                let short = p.name.rsplit('.').next().unwrap_or(&p.name).to_lowercase();
                if let Some(icon) = icon_map.get(&short).or_else(|| icon_map.get(&p.display_title().to_lowercase())) {
                    p.icon = Some(icon.clone());
                }
            }
        }

        Ok(all_pkgs)
    }

    pub fn deduplicate_packages(packages: Vec<Package>) -> Vec<Package> {
        let mut deduped = Vec::new();
        let mut seen_keys = std::collections::HashSet::new();

        for p in packages {
            let clean = p.name.strip_suffix(".desktop").unwrap_or(&p.name);
            let short = clean.rsplit('.').next().unwrap_or(clean).to_lowercase();
            let title = p.display_title().to_lowercase().trim().to_string();

            let is_dup = if !short.is_empty() && seen_keys.contains(&format!("short:{}", short)) {
                true
            } else if !title.is_empty() && title != "application" && title != "package" && seen_keys.contains(&format!("title:{}", title)) {
                true
            } else {
                false
            };

            if !is_dup {
                if !short.is_empty() {
                    seen_keys.insert(format!("short:{}", short));
                }
                if !title.is_empty() && title != "application" && title != "package" {
                    seen_keys.insert(format!("title:{}", title));
                }
                deduped.push(p);
            }
        }

        deduped
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Package>, PastorError> {
        let all_pkgs = self.search_raw(query).await?;
        Ok(Self::deduplicate_packages(all_pkgs))
    }

    pub async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError> {
        for b in &self.backends {
            if let Ok(Some(pkg)) = b.get_package(id).await {
                return Ok(Some(pkg));
            }
        }
        Ok(None)
    }

    pub async fn get_installed(&self) -> Result<Vec<Package>, PastorError> {
        let mut futures = Vec::new();
        for b in &self.backends {
            futures.push(b.installed());
        }

        let results = join_all(futures).await;
        let mut installed = Vec::new();
        let config = self.module_config();
        for res in results {
            if let Ok(pkgs) = res {
                for p in pkgs {
                    if config.is_source_enabled(&p.id.source) {
                        installed.push(p);
                    }
                }
            }
        }
        // Check for available updates to mark PackageState::UpdateAvailable
        if let Ok(updates) = self.get_updates().await {
            let update_ids: std::collections::HashSet<String> = updates.into_iter().map(|u| u.id.name).collect();
            for p in &mut installed {
                let clean = p.id.name.strip_suffix(".desktop").unwrap_or(&p.id.name);
                if update_ids.contains(&p.id.name) || update_ids.contains(&p.name) || update_ids.contains(clean) {
                    p.state = PackageState::UpdateAvailable;
                }
            }
        }

        Ok(installed)
    }

    pub async fn get_updates(&self) -> Result<Vec<PackageUpdate>, PastorError> {
        let mut futures = Vec::new();
        for b in &self.backends {
            futures.push(b.updates());
        }

        let results = join_all(futures).await;
        let mut updates = Vec::new();
        let config = self.module_config();
        for res in results {
            if let Ok(up_list) = res {
                for up in up_list {
                    if config.is_source_enabled(&up.id.source) {
                        updates.push(up);
                    }
                }
            }
        }
        Ok(updates)
    }

    pub async fn get_by_category(&self, category: PackageCategory) -> Result<Vec<Package>, PastorError> {
        let mut futures = Vec::new();
        for b in &self.backends {
            futures.push(b.get_by_category(category));
        }

        let results = join_all(futures).await;
        let mut all_pkgs = Vec::new();
        let config = self.module_config();

        for res in results {
            match res {
                Ok(pkgs) => {
                    for p in pkgs {
                        if config.is_source_enabled(&p.id.source) {
                            all_pkgs.push(p);
                        }
                    }
                }
                Err(e) => tracing::warn!("Backend get_by_category error: {e}"),
            }
        }
        Ok(Self::deduplicate_packages(all_pkgs))
    }

    pub async fn get_curated_picks(&self) -> Result<Vec<Package>, PastorError> {
        let mut futures = Vec::new();
        for b in &self.backends {
            futures.push(b.curated_picks());
        }

        let results = join_all(futures).await;
        let mut all_pkgs = Vec::new();
        let config = self.module_config();

        for res in results {
            match res {
                Ok(pkgs) => {
                    for p in pkgs {
                        if config.is_source_enabled(&p.id.source) {
                            all_pkgs.push(p);
                        }
                    }
                }
                Err(e) => tracing::warn!("Backend curated_picks error: {e}"),
            }
        }
        Ok(Self::deduplicate_packages(all_pkgs))
    }

    pub async fn get_alternatives(&self, package_name: &str) -> Result<Vec<Package>, PastorError> {
        let clean = package_name.strip_suffix(".desktop").unwrap_or(package_name);
        let short_name = clean.rsplit('.').next().unwrap_or(clean);

        if short_name.is_empty() {
            return Ok(vec![]);
        }

        let matching = self.search_raw(short_name).await?;
        let mut seen_sources = std::collections::HashSet::new();
        let mut alternatives = Vec::new();

        for p in matching {
            let p_clean = p.name.strip_suffix(".desktop").unwrap_or(&p.name);
            let p_short = p_clean.rsplit('.').next().unwrap_or(p_clean);

            let is_match = p.name.eq_ignore_ascii_case(package_name)
                || p.name.eq_ignore_ascii_case(clean)
                || p_clean.eq_ignore_ascii_case(clean)
                || p_short.eq_ignore_ascii_case(short_name)
                || p.display_title().eq_ignore_ascii_case(short_name);

            if is_match {
                let source_key = match &p.id.source {
                    PackageSource::Flatpak { .. } => "flatpak".to_string(),
                    PackageSource::Arch(_) => "arch".to_string(),
                    PackageSource::Parch(_) => "parch".to_string(),
                    PackageSource::Aur => "aur".to_string(),
                    PackageSource::Bootc => "bootc".to_string(),
                    PackageSource::Waydroid => "waydroid".to_string(),
                };
                if seen_sources.insert(source_key) {
                    alternatives.push(p);
                }
            }
        }

        Ok(alternatives)
    }

    pub fn subscribe_transactions(&self) -> broadcast::Receiver<ActiveTransactionEvent> {
        self.tx_broadcaster.subscribe()
    }

    pub fn get_active_transactions(&self) -> Vec<ActiveTransactionInfo> {
        self.active_transactions
            .read()
            .map(|guard| guard.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_active_transaction(&self, id: &PackageId) -> Option<ActiveTransactionInfo> {
        self.active_transactions.read().ok().and_then(|guard| {
            if let Some(info) = guard.get(id) {
                return Some(info.clone());
            }
            let clean_name = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
            for (k, v) in guard.iter() {
                let k_clean = k.name.strip_suffix(".desktop").unwrap_or(&k.name);
                if k_clean == clean_name {
                    return Some(v.clone());
                }
            }
            None
        })
    }

    pub fn is_installing(&self, id: &PackageId) -> bool {
        self.get_active_transaction(id).is_some()
    }

    pub async fn cancel_installation(&self, id: &PackageId) -> Result<(), PastorError> {
        for b in &self.backends {
            let _ = b.cancel(id).await;
        }
        let mut was_active = false;
        if let Ok(mut guard) = self.active_transactions.write() {
            if guard.remove(id).is_some() {
                was_active = true;
            } else {
                let clean_name = id.name.strip_suffix(".desktop").unwrap_or(&id.name);
                let to_remove: Vec<PackageId> = guard
                    .keys()
                    .filter(|k| k.name.strip_suffix(".desktop").unwrap_or(&k.name) == clean_name)
                    .cloned()
                    .collect();
                for k in to_remove {
                    guard.remove(&k);
                    was_active = true;
                }
            }
        }
        if was_active {
            let _ = self.tx_broadcaster.send(ActiveTransactionEvent::Cancelled(id.clone()));
        }
        Ok(())
    }

    /// Trigger installation and return a stream of transaction events
    pub fn install(&self, id: PackageId) -> Receiver<TransactionEvent> {
        let (tx, rx): (Sender<TransactionEvent>, Receiver<TransactionEvent>) = mpsc::channel(100);
        let backends = self.backends.clone();
        let active_map = self.active_transactions.clone();
        let broadcaster = self.tx_broadcaster.clone();

        let pkg_name = id.name.clone();
        let initial_info = ActiveTransactionInfo {
            package_id: id.clone(),
            package_name: pkg_name.clone(),
            progress_fraction: 0.0,
            step: TransactionStep::Idle,
            log_message: format!("Starting installation of {}...", pkg_name),
        };

        if let Ok(mut guard) = active_map.write() {
            guard.insert(id.clone(), initial_info.clone());
        }
        let _ = broadcaster.send(ActiveTransactionEvent::Started(initial_info));

        tokio::spawn(async move {
            let (backend_tx, mut backend_rx) = mpsc::channel::<TransactionEvent>(100);
            let b_clone = backends.iter().filter(|b| b.handles(&id)).cloned().collect::<Vec<_>>();
            let id_clone = id.clone();

            if b_clone.is_empty() {
                let failed_msg = format!(
                    "No package backend handles '{}' ({})",
                    id_clone.name,
                    id_clone.source.display_badge(),
                );
                let failed = TransactionEvent {
                    step: TransactionStep::Failed(failed_msg.clone()),
                    progress_fraction: 1.0,
                    log_message: format!("No package backend handles '{}'", id_clone.name),
                };
                if let Ok(mut guard) = active_map.write() {
                    guard.remove(&id_clone);
                }
                let _ = broadcaster.send(ActiveTransactionEvent::Failed(id_clone.clone(), failed_msg));
                let _ = tx.send(failed).await;
                return;
            }

            let join_handle = tokio::spawn(async move {
                for b in b_clone {
                    if let Err(e) = b.install(&id_clone, backend_tx.clone()).await {
                        let _ = backend_tx
                            .send(TransactionEvent {
                                step: TransactionStep::Failed(e.to_string()),
                                progress_fraction: 1.0,
                                log_message: format!("Error installing: {e}"),
                            })
                            .await;
                    }
                }
            });

            while let Some(evt) = backend_rx.recv().await {
                match &evt.step {
                    TransactionStep::Completed => {
                        if let Ok(mut guard) = active_map.write() {
                            guard.remove(&id);
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Completed(id.clone()));
                    }
                    TransactionStep::Cancelled => {
                        if let Ok(mut guard) = active_map.write() {
                            guard.remove(&id);
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Cancelled(id.clone()));
                    }
                    TransactionStep::Failed(err) => {
                        if let Ok(mut guard) = active_map.write() {
                            guard.remove(&id);
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Failed(id.clone(), err.clone()));
                    }
                    _ => {
                        let info = ActiveTransactionInfo {
                            package_id: id.clone(),
                            package_name: pkg_name.clone(),
                            progress_fraction: evt.progress_fraction,
                            step: evt.step.clone(),
                            log_message: evt.log_message.clone(),
                        };
                        if let Ok(mut guard) = active_map.write() {
                            guard.insert(id.clone(), info.clone());
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Progress(info));
                    }
                }

                let _ = tx.send(evt).await;
            }

            let _ = join_handle.await;

            if let Ok(mut guard) = active_map.write() {
                guard.remove(&id);
            }
        });

        rx
    }

    /// Trigger removal and return a stream of transaction events
    pub fn remove(&self, id: PackageId) -> Receiver<TransactionEvent> {
        let (tx, rx): (Sender<TransactionEvent>, Receiver<TransactionEvent>) = mpsc::channel(100);
        let backends = self.backends.clone();
        let active_map = self.active_transactions.clone();
        let broadcaster = self.tx_broadcaster.clone();

        let pkg_name = id.name.clone();
        let initial_info = ActiveTransactionInfo {
            package_id: id.clone(),
            package_name: pkg_name.clone(),
            progress_fraction: 0.0,
            step: TransactionStep::ApplyingChanges {
                package: pkg_name.clone(),
                index: 1,
                total: 1,
            },
            log_message: format!("Queueing removal of {}...", pkg_name),
        };

        if let Ok(mut guard) = active_map.write() {
            guard.insert(id.clone(), initial_info.clone());
        }
        let _ = broadcaster.send(ActiveTransactionEvent::Started(initial_info));

        tokio::spawn(async move {
            let (backend_tx, mut backend_rx) = mpsc::channel::<TransactionEvent>(100);
            let b_clone = backends.iter().filter(|b| b.handles(&id)).cloned().collect::<Vec<_>>();
            let id_clone = id.clone();

            if b_clone.is_empty() {
                let failed_msg = format!(
                    "No package backend handles '{}' ({})",
                    id_clone.name,
                    id_clone.source.display_badge(),
                );
                let failed = TransactionEvent {
                    step: TransactionStep::Failed(failed_msg.clone()),
                    progress_fraction: 1.0,
                    log_message: format!("No package backend handles '{}'", id_clone.name),
                };
                if let Ok(mut guard) = active_map.write() {
                    guard.remove(&id_clone);
                }
                let _ = broadcaster.send(ActiveTransactionEvent::Failed(id_clone.clone(), failed_msg));
                let _ = tx.send(failed).await;
                return;
            }

            let join_handle = tokio::spawn(async move {
                for b in b_clone {
                    if let Err(e) = b.remove(&id_clone, backend_tx.clone()).await {
                        let _ = backend_tx
                            .send(TransactionEvent {
                                step: TransactionStep::Failed(e.to_string()),
                                progress_fraction: 1.0,
                                log_message: format!("Error removing: {e}"),
                            })
                            .await;
                    }
                }
            });

            while let Some(evt) = backend_rx.recv().await {
                match &evt.step {
                    TransactionStep::Completed => {
                        if let Ok(mut guard) = active_map.write() {
                            guard.remove(&id);
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Completed(id.clone()));
                    }
                    TransactionStep::Cancelled => {
                        if let Ok(mut guard) = active_map.write() {
                            guard.remove(&id);
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Cancelled(id.clone()));
                    }
                    TransactionStep::Failed(err) => {
                        if let Ok(mut guard) = active_map.write() {
                            guard.remove(&id);
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Failed(id.clone(), err.clone()));
                    }
                    _ => {
                        let info = ActiveTransactionInfo {
                            package_id: id.clone(),
                            package_name: pkg_name.clone(),
                            progress_fraction: evt.progress_fraction,
                            step: evt.step.clone(),
                            log_message: evt.log_message.clone(),
                        };
                        if let Ok(mut guard) = active_map.write() {
                            guard.insert(id.clone(), info.clone());
                        }
                        let _ = broadcaster.send(ActiveTransactionEvent::Progress(info));
                    }
                }

                let _ = tx.send(evt).await;
            }

            let _ = join_handle.await;

            if let Ok(mut guard) = active_map.write() {
                guard.remove(&id);
            }
        });

        rx
    }

    /// Retrieve available downgrade versions
    pub async fn get_downgrade_versions(&self, id: &PackageId) -> Result<Vec<String>, PastorError> {
        for b in &self.backends {
            if let Ok(versions) = b.downgrade_versions(id).await {
                if !versions.is_empty() {
                    return Ok(versions);
                }
            }
        }
        Ok(vec![])
    }

    /// Trigger downgrade to target version
    pub fn downgrade(&self, id: PackageId, target_version: String) -> Receiver<TransactionEvent> {
        let (tx, rx): (Sender<TransactionEvent>, Receiver<TransactionEvent>) = mpsc::channel(100);
        let backends = self.backends.clone();

        tokio::spawn(async move {
            for b in backends.iter().filter(|b| b.handles(&id)) {
                if let Err(e) = b.downgrade(&id, &target_version, tx.clone()).await {
                    let _ = tx
                        .send(TransactionEvent {
                            step: TransactionStep::Failed(e.to_string()),
                            progress_fraction: 1.0,
                            log_message: format!("Error downgrading: {e}"),
                        })
                        .await;
                }
            }
        });

        rx
    }
}
