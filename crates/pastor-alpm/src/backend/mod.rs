pub mod builder;
pub mod privileged;
pub mod search;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};

use async_trait::async_trait;
use pastor_core::{
    backend::PackageBackend,
    error::PastorError,
    package::{Package, PackageCategory, PackageId, PackageSource, PackageUpdate},
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
        let _test_alpm = privileged::create_alpm(&pacman_config)?;

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

    pub fn get_installed_pkg_version(&self, name: &str) -> Option<String> {
        search::get_installed_pkg_version(&self.pacman_config, name)
    }

    pub fn get_foreign_packages(&self) -> Vec<(String, String)> {
        search::get_foreign_packages(&self.pacman_config)
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
            privileged::run_privileged("install-file", &[&path_str], progress_tx)
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
            privileged::run_privileged("remove", &[&name_str], progress_tx)
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
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();
        let q = query.to_string();

        tokio::task::spawn_blocking(move || {
            let catalog = catalog_arc.read().unwrap();
            search::search(&pacman, &catalog, &q)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("search task failed: {e}"),
        })?
    }

    async fn get_by_category(&self, category: PackageCategory) -> Result<Vec<Package>, PastorError> {
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();

        tokio::task::spawn_blocking(move || {
            let catalog = catalog_arc.read().unwrap();
            search::get_by_category(&pacman, &catalog, category)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("get_by_category task failed: {e}"),
        })?
    }

    async fn curated_picks(&self) -> Result<Vec<Package>, PastorError> {
        Ok(vec![])
    }

    async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError> {
        if !self.handles(id) {
            return Ok(None);
        }

        let name = id.name.clone();
        let pacman = self.pacman_config.clone();
        let catalog_arc = self.catalog.clone();

        tokio::task::spawn_blocking(move || {
            let catalog = catalog_arc.read().unwrap();
            search::get_package(&pacman, &catalog, &name)
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

        tokio::task::spawn_blocking(move || {
            let catalog = catalog_arc.read().unwrap();
            search::installed(&pacman, &catalog)
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
            search::updates(&pacman)
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

        let deps = tokio::task::spawn_blocking({
            let pacman = pacman.clone();
            let target_name = target_name.clone();
            move || privileged::detect_dependencies_internal(&pacman, &[&target_name])
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

        let tx_clone = progress_tx.clone();
        tokio::task::spawn_blocking(move || {
            privileged::run_privileged("install", &[&target_name], tx_clone)
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
            privileged::run_privileged("remove", &[&target_name], tx_clone)
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
            privileged::run_privileged("upgrade", &[], progress_tx)
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
            privileged::run_privileged("refresh", &[], progress_tx)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "alpm".into(),
            message: format!("Refresh databases worker join error: {e}"),
        })?
    }

    async fn downgrade_versions(&self, id: &PackageId) -> Result<Vec<String>, PastorError> {
        let pkg_name = id.name.clone();
        let cache_dir = builder::get_cache_dir(&self.pacman_config);

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
        let cache_dir = builder::get_cache_dir(&self.pacman_config);

        let pkg_path = tokio::task::spawn_blocking(move || {
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
            privileged::run_privileged("downgrade", &[&path_str], progress_tx)
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
