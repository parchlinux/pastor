use std::sync::{Arc, RwLock};
use async_trait::async_trait;
use pastor_alpm::AlpmBackend;
use pastor_core::{
    backend::PackageBackend,
    error::PastorError,
    package::{Package, PackageId, PackageSource, PackageState, PackageUpdate},
    transaction::{TransactionEvent, TransactionStep},
};
use tokio::sync::mpsc::Sender;

use crate::{
    api::{rpc_pkg_to_package, AurClient},
    git::AurGitRepo,
    sandbox::BwrapSandbox,
    scanner::{HeuristicScanner, ScanReport},
    state::AurStateStore,
    trust::TrustReport,
    verifier::ArtifactVerifier,
};

pub struct AurBackend {
    name: &'static str,
    client: AurClient,
    alpm: Arc<AlpmBackend>,
    state_store: Arc<RwLock<AurStateStore>>,
    skip_sandbox_packages: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl AurBackend {
    pub fn new(alpm: Arc<AlpmBackend>) -> Self {
        Self {
            name: "aur",
            client: AurClient::new(),
            alpm,
            state_store: Arc::new(RwLock::new(AurStateStore::load())),
            skip_sandbox_packages: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    pub fn client(&self) -> &AurClient {
        &self.client
    }

    pub fn state_store(&self) -> Arc<RwLock<AurStateStore>> {
        self.state_store.clone()
    }

    pub fn set_sandbox_for_package(&self, pkg_name: &str, enabled: bool) {
        if let Ok(mut set) = self.skip_sandbox_packages.write() {
            if enabled {
                set.remove(pkg_name);
            } else {
                set.insert(pkg_name.to_string());
            }
        }
    }

    /// Perform pre-build security analysis and git synchronization
    pub async fn prepare_and_scan(
        &self,
        package_name: &str,
    ) -> Result<(String, TrustReport, ScanReport, String), PastorError> {
        let repo = AurGitRepo::for_package(package_name);
        let current_commit = repo.sync()?;

        let rpc_info = self.client.info(package_name).await?;
        let maintainer = rpc_info.as_ref().and_then(|r| r.maintainer.as_deref());

        let store = self.state_store.read().unwrap().clone();
        let trust = TrustReport::assess(package_name, maintainer, &store);

        let pkgbuild_path = repo.repo_dir.join("PKGBUILD");
        let mut findings = HeuristicScanner::scan_file(&pkgbuild_path)?;
        for inst_file in repo.find_install_files() {
            findings.extend(HeuristicScanner::scan_file(&inst_file)?);
        }

        let base_commit = store.get_record(package_name).map(|r| r.last_installed_commit.as_str());
        let diff = repo.get_diff(base_commit)?;

        let scan_report = ScanReport { findings };
        Ok((current_commit, trust, scan_report, diff))
    }
}

#[async_trait]
impl PackageBackend for AurBackend {
    fn name(&self) -> &'static str {
        self.name
    }

    fn handles(&self, id: &PackageId) -> bool {
        matches!(id.source, PackageSource::Aur)
    }

    async fn search(&self, query: &str) -> Result<Vec<Package>, PastorError> {
        let rpc_results = self.client.search(query).await?;
        let mut packages = Vec::new();

        for rpc in rpc_results {
            let installed_ver = self.alpm.get_installed_pkg_version(&rpc.name);
            packages.push(rpc_pkg_to_package(rpc, installed_ver));
        }

        Ok(packages)
    }

    async fn get_package(&self, id: &PackageId) -> Result<Option<Package>, PastorError> {
        if !self.handles(id) {
            return Ok(None);
        }

        let rpc_opt = self.client.info(&id.name).await?;
        if let Some(rpc) = rpc_opt {
            let installed_ver = self.alpm.get_installed_pkg_version(&rpc.name);
            Ok(Some(rpc_pkg_to_package(rpc, installed_ver)))
        } else {
            Ok(None)
        }
    }

    async fn installed(&self) -> Result<Vec<Package>, PastorError> {
        let foreign_pkgs = self.alpm.get_foreign_packages();
        let store = self.state_store.read().unwrap().clone();

        let mut pkg_map = std::collections::HashMap::new();
        for (name, ver) in foreign_pkgs {
            pkg_map.insert(name, ver);
        }
        for (name, _rec) in store.packages {
            if !pkg_map.contains_key(&name) {
                if let Some(ver) = self.alpm.get_installed_pkg_version(&name) {
                    pkg_map.insert(name, ver);
                }
            }
        }

        if pkg_map.is_empty() {
            return Ok(vec![]);
        }

        let names: Vec<String> = pkg_map.keys().cloned().collect();
        let rpc_pkgs = self.client.multi_info(&names).await.unwrap_or_default();

        let mut pkgs = Vec::new();
        let mut found_names = std::collections::HashSet::new();

        for rpc in rpc_pkgs {
            found_names.insert(rpc.name.clone());
            let installed_ver = pkg_map.get(&rpc.name).cloned();
            pkgs.push(rpc_pkg_to_package(rpc, installed_ver));
        }

        // For foreign packages not found on AUR (e.g. offline/custom pkgs)
        for (name, ver) in &pkg_map {
            if !found_names.contains(name) {
                pkgs.push(Package {
                    id: PackageId::new(name, PackageSource::Aur),
                    name: name.clone(),
                    display_name: Some(name.clone()),
                    version: ver.clone(),
                    installed_version: Some(ver.clone()),
                    summary: "Foreign / Local package".to_string(),
                    description: None,
                    icon: Some(pastor_core::package::PackageIcon::Themed("package-x-generic".to_string())),
                    screenshots: vec![],
                    homepage: None,
                    license: None,
                    maintainer: None,
                    categories: vec![pastor_core::package::PackageCategory::Utilities],
                    size_installed: None,
                    size_download: None,
                    dependencies: vec![],
                    changelog: None,
                    state: PackageState::Installed,
                });
            }
        }

        Ok(pkgs)
    }

    async fn updates(&self) -> Result<Vec<PackageUpdate>, PastorError> {
        let foreign_pkgs = self.alpm.get_foreign_packages();
        let store = self.state_store.read().unwrap().clone();

        let mut pkg_map = std::collections::HashMap::new();
        for (name, ver) in foreign_pkgs {
            pkg_map.insert(name, ver);
        }
        for (name, _rec) in store.packages {
            if !pkg_map.contains_key(&name) {
                if let Some(ver) = self.alpm.get_installed_pkg_version(&name) {
                    pkg_map.insert(name, ver);
                }
            }
        }

        if pkg_map.is_empty() {
            return Ok(vec![]);
        }

        let names: Vec<String> = pkg_map.keys().cloned().collect();
        let rpc_pkgs = self.client.multi_info(&names).await.unwrap_or_default();

        let mut updates = Vec::new();
        for rpc in rpc_pkgs {
            if let Some(inst_ver) = pkg_map.get(&rpc.name) {
                if crate::api::alpm_vercmp(&rpc.version, inst_ver) > 0 {
                    updates.push(PackageUpdate {
                        id: PackageId::new(rpc.name.clone(), PackageSource::Aur),
                        current_version: inst_ver.clone(),
                        new_version: rpc.version.clone(),
                        download_size: None,
                        changelog: rpc.description,
                    });
                }
            }
        }

        Ok(updates)
    }

    async fn update_all(
        &self,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let ups = self.updates().await?;
        let total = ups.len();
        for (idx, up) in ups.into_iter().enumerate() {
            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::ApplyingChanges {
                        package: up.id.name.clone(),
                        index: idx + 1,
                        total,
                    },
                    progress_fraction: (idx as f32) / (total as f32),
                    log_message: format!("Updating AUR package {} ({}/{})", up.id.name, idx + 1, total),
                })
                .await;

            self.install(&up.id, progress_tx.clone()).await?;
        }
        Ok(())
    }

    async fn install(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_name = id.name.clone();

        // 1. Synchronize Git repository and assess Trust
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::SyncingDatabase,
                progress_fraction: 0.05,
                log_message: format!("Fetching AUR git repository for '{}'...", pkg_name),
            })
            .await;

        let repo = AurGitRepo::for_package(&pkg_name);
        let current_commit = repo.sync()?;

        let rpc_info = self.client.info(&pkg_name).await?;
        let maintainer = rpc_info.as_ref().and_then(|r| r.maintainer.as_deref());

        let trust = {
            let store = self.state_store.read().unwrap();
            TrustReport::assess(&pkg_name, maintainer, &store)
        };

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::CheckingIntegrity,
                progress_fraction: 0.15,
                log_message: format!("Trust status: {}", trust.title),
            })
            .await;

        // 2. Static Heuristic Scanner on PKGBUILD & .install
        let pkgbuild_path = repo.repo_dir.join("PKGBUILD");
        let mut findings = HeuristicScanner::scan_file(&pkgbuild_path)?;
        for inst_file in repo.find_install_files() {
            findings.extend(HeuristicScanner::scan_file(&inst_file)?);
        }

        let scan_report = ScanReport { findings };
        if scan_report.has_critical() {
            let critical_desc = scan_report
                .findings
                .iter()
                .filter(|f| f.severity == crate::scanner::RuleSeverity::Critical)
                .map(|f| format!("{}: {}", f.title, f.line_content))
                .collect::<Vec<_>>()
                .join(" | ");

            let err = format!(
                "AUR Security Alert: Critical threat detected in '{}' PKGBUILD / .install ({})",
                pkg_name, critical_desc
            );
            tracing::error!("{}", err);
            return Err(PastorError::TransactionFailed(err));
        }

        // 3. Check sandbox preference (Bubblewrap vs Host)
        let use_sandbox = {
            let mut guard = self.skip_sandbox_packages.write().unwrap();
            !guard.remove(&pkg_name)
        };

        let compiled_pkg_path = if use_sandbox {
            if !BwrapSandbox::is_available() {
                return Err(PastorError::BackendError {
                    backend: "aur".into(),
                    message: "bubblewrap (bwrap) sandbox utility is not available on host system".into(),
                });
            }

            // Phase A: Fetch upstream sources (network allowed)
            BwrapSandbox::run_phase_a_fetch(&repo.repo_dir, progress_tx.clone()).await?;

            // Phase B: Sandboxed build and package (ZERO NETWORK ACCESS)
            BwrapSandbox::run_phase_b_build(&repo.repo_dir, progress_tx.clone()).await?
        } else {
            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::ApplyingChanges {
                        package: pkg_name.clone(),
                        index: 1,
                        total: 1,
                    },
                    progress_fraction: 0.4,
                    log_message: "Compiling AUR package on host (Sandbox disabled by user request)...".to_string(),
                })
                .await;

            let bdir = repo.repo_dir.clone();
            tokio::task::spawn_blocking(move || {
                let status = std::process::Command::new("makepkg")
                    .current_dir(&bdir)
                    .args(["--nobuild", "--noconfirm", "--nodeps"])
                    .status()
                    .map_err(|e| PastorError::BackendError {
                        backend: "aur".into(),
                        message: format!("Host source fetch failed: {e}"),
                    })?;

                if !status.success() {
                    return Err(PastorError::BackendError {
                        backend: "aur".into(),
                        message: "Host makepkg source fetch returned non-zero status".into(),
                    });
                }

                let status_build = std::process::Command::new("makepkg")
                    .current_dir(&bdir)
                    .args(["--noextract", "--nocheck", "--noconfirm", "--nodeps"])
                    .status()
                    .map_err(|e| PastorError::BackendError {
                        backend: "aur".into(),
                        message: format!("Host compilation failed: {e}"),
                    })?;

                if !status_build.success() {
                    return Err(PastorError::BackendError {
                        backend: "aur".into(),
                        message: "Host makepkg build returned non-zero status".into(),
                    });
                }

                let mut found = None;
                if let Ok(entries) = std::fs::read_dir(&bdir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.contains(".pkg.tar.") {
                            found = Some(path);
                            break;
                        }
                    }
                }

                found.ok_or_else(|| PastorError::BackendError {
                    backend: "aur".into(),
                    message: "No compiled .pkg.tar artifact found after host makepkg".into(),
                })
            })
            .await
            .map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Host makepkg task error: {e}"),
            })??
        };

        // 4. Pre-Pacman verification of compiled package hooks
        let _ = ArtifactVerifier::verify_package_install_hook(&compiled_pkg_path)?;

        // 5. Install compiled artifact via ALPM privileged worker
        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: pkg_name.clone(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.9,
                log_message: format!(
                    "Installing verified package {} with ALPM...",
                    compiled_pkg_path.file_name().and_then(|n| n.to_str()).unwrap_or("artifact")
                ),
            })
            .await;

        self.alpm
            .install_local_package(&compiled_pkg_path, progress_tx.clone())
            .await?;

        // 6. Record successful install in local state
        if let Ok(mut store) = self.state_store.write() {
            let _ = store.record_installed(&pkg_name, &current_commit, maintainer);
        }

        let _ = progress_tx
            .send(TransactionEvent {
                step: TransactionStep::Completed,
                progress_fraction: 1.0,
                log_message: format!("Successfully installed AUR package '{}'", pkg_name),
            })
            .await;

        Ok(())
    }

    async fn remove(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_name = id.name.clone();
        self.alpm.remove_package_by_name(&pkg_name, progress_tx).await?;

        if let Ok(mut store) = self.state_store.write() {
            let _ = store.remove_record(&pkg_name);
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
        Err(PastorError::BackendError {
            backend: "aur".into(),
            message: "Direct AUR downgrade not implemented; use local ALPM package cache".into(),
        })
    }
}
