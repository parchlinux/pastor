use std::collections::HashSet;
use std::sync::Arc;
use parking_lot::RwLock;
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
    operation_lock: Arc<tokio::sync::Mutex<()>>,
    /// Package names queued for cancellation; checked at every phase boundary
    /// of the install pipeline (fetch, scan, deps, build, artifact install).
    cancellations: Arc<RwLock<HashSet<String>>>,
}

impl AurBackend {
    pub fn new(alpm: Arc<AlpmBackend>) -> Self {
        Self {
            name: "aur",
            client: AurClient::new(),
            alpm,
            state_store: Arc::new(RwLock::new(AurStateStore::load())),
            operation_lock: Arc::new(tokio::sync::Mutex::new(())),
            cancellations: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn client(&self) -> &AurClient {
        &self.client
    }

    pub fn state_store(&self) -> Arc<RwLock<AurStateStore>> {
        self.state_store.clone()
    }

    pub fn set_sandbox_for_package(&self, _pkg_name: &str, _enabled: bool) {
        // Bubblewrap sandbox is intentionally disabled per user configuration
    }

    /// Perform pre-build security analysis and git synchronization. Also returns
    /// the missing official-repo dependencies that the install pipeline would
    /// install with elevated privileges, so the UI can show them for approval.
    pub async fn prepare_and_scan(
        &self,
        package_name: &str,
    ) -> Result<(String, TrustReport, ScanReport, String, Vec<String>), PastorError> {
        let repo = AurGitRepo::for_package(package_name);
        let current_commit = repo.sync()?;

        let rpc_info = self.client.info(package_name).await?;
        let maintainer = rpc_info.as_ref().and_then(|r| r.maintainer.as_deref());

        let store = self.state_store.read().clone();
        let trust = TrustReport::assess(package_name, maintainer, &store);

        let pkgbuild_path = repo.repo_dir.join("PKGBUILD");
        let mut findings = HeuristicScanner::scan_file(&pkgbuild_path)?;
        for inst_file in repo.find_install_files() {
            findings.extend(HeuristicScanner::scan_file(&inst_file)?);
        }

        let base_commit = store.get_record(package_name).map(|r| r.last_installed_commit.as_str());
        let diff = repo.get_diff(base_commit)?;

        let all_deps = extract_dependencies(&repo.repo_dir);
        let (missing_sync, _missing_other) = self.alpm.find_missing_sync_dependencies(&all_deps);

        let scan_report = ScanReport { findings };
        Ok((current_commit, trust, scan_report, diff, missing_sync))
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
        let store = self.state_store.read().clone();

        let mut pkg_map = std::collections::HashMap::new();
        for (name, ver) in foreign_pkgs {
            pkg_map.insert(name, ver);
        }

        let mut stale_packages = Vec::new();
        for (name, _rec) in &store.packages {
            if !pkg_map.contains_key(name) {
                if let Some(ver) = self.alpm.get_installed_pkg_version(name) {
                    pkg_map.insert(name.clone(), ver);
                } else {
                    stale_packages.push(name.clone());
                }
            }
        }

        if !stale_packages.is_empty() {
            let mut write_guard = self.state_store.write();
            for stale in stale_packages {
                let _ = write_guard.remove_record(&stale);
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
        let store = self.state_store.read().clone();

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
        let rpc_pkgs = self.client.multi_info(&names).await?;

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
        let mut errs: Vec<String> = Vec::new();
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

            // Continue past failures so one broken package doesn't abandon the
            // rest of the batch; errors are aggregated into one terminal event.
            if let Err(e) = self.install(&up.id, progress_tx.clone()).await {
                errs.push(format!("{}: {}", up.id.name, e));
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(PastorError::TransactionFailed(errs.join(" | ")))
        }
    }

    async fn install(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_name = id.name.clone();

        // Check if an operation is already running; if so, notify the UI that this package is queued
        let is_busy = self.operation_lock.try_lock().is_err();
        if is_busy {
            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::SyncingDatabase,
                    progress_fraction: 0.0,
                    log_message: format!("[{}] Queued — waiting for active transaction to finish...", pkg_name),
                })
                .await;
        }

        // Acquire lock so AUR operations queue strictly one after another
        let _permit = self.operation_lock.lock().await;

        let mut visited = std::collections::HashSet::new();
        self.install_internal(id, progress_tx, &mut visited).await
    }

    async fn remove(
        &self,
        id: &PackageId,
        progress_tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let pkg_name = id.name.clone();
        let _permit = self.operation_lock.lock().await;
        self.alpm.remove_package_by_name(&pkg_name, progress_tx).await?;

        let _ = self.state_store.write().remove_record(&pkg_name);

        Ok(())
    }

    /// Queue cancellation for an AUR package. The flag is honoured at every
    /// phase boundary of `install_internal` (fetch, scan, deps, build), so a
    /// queued or in-flight build stops before its next elevated step. A makepkg
    /// already running cannot be killed safely; it will run to completion but
    /// its artifact is not installed afterwards.
    async fn cancel(&self, id: &PackageId) -> Result<(), PastorError> {
        self.cancellations.write().insert(id.name.clone());
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

impl AurBackend {
    fn install_internal<'a>(
        &'a self,
        id: &'a PackageId,
        progress_tx: Sender<TransactionEvent>,
        visited: &'a mut std::collections::HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), PastorError>> + Send + 'a>> {
        Box::pin(async move {
            let pkg_name = id.name.clone();

            // Cancellation check helper: honoured at every phase boundary so a
            // queued or in-flight build stops before the next root-reaching step.
            let cancelled = |name: &str| self.cancellations.read().contains(name);
            if cancelled(&pkg_name) {
                self.cancellations.write().remove(&pkg_name);
                return Err(PastorError::Cancelled(format!(
                    "Installation of '{pkg_name}' was cancelled"
                )));
            }

            if visited.contains(&pkg_name) {
                return Err(PastorError::TransactionFailed(format!(
                    "Circular dependency loop detected involving AUR package '{pkg_name}'"
                )));
            }
            visited.insert(pkg_name.clone());

            // 1. Synchronize Git repository and assess Trust
            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::SyncingDatabase,
                    progress_fraction: 0.05,
                    log_message: format!("[{}] Fetching AUR git repository...", pkg_name),
                })
                .await;

            let repo = AurGitRepo::for_package(&pkg_name);
            let current_commit = repo.sync()?;

            if cancelled(&pkg_name) {
                self.cancellations.write().remove(&pkg_name);
                return Err(PastorError::Cancelled(format!(
                    "Installation of '{pkg_name}' was cancelled"
                )));
            }

            let rpc_info = self.client.info(&pkg_name).await?;
            let maintainer = rpc_info.as_ref().and_then(|r| r.maintainer.as_deref());

            let trust = {
                let store = self.state_store.read();
                TrustReport::assess(&pkg_name, maintainer, &store)
            };

            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::CheckingIntegrity,
                    progress_fraction: 0.15,
                    log_message: format!("[{}] Trust status: {}", pkg_name, trust.title),
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

            // 2b. Extract dependencies and install missing ones using ALPM worker
            let all_deps = extract_dependencies(&repo.repo_dir);
            let (missing_sync, missing_other) = self.alpm.find_missing_sync_dependencies(&all_deps);

            if cancelled(&pkg_name) {
                self.cancellations.write().remove(&pkg_name);
                return Err(PastorError::Cancelled(format!(
                    "Installation of '{pkg_name}' was cancelled"
                )));
            }

            if !missing_sync.is_empty() {
                let _ = progress_tx
                    .send(TransactionEvent {
                        step: TransactionStep::CheckingDependencies,
                        progress_fraction: 0.2,
                        log_message: format!(
                            "[{}] Installing {} missing sync dependency/dependencies with ALPM: {}",
                            pkg_name,
                            missing_sync.len(),
                            missing_sync.join(", ")
                        ),
                    })
                    .await;

                self.alpm
                    .install_sync_packages(&missing_sync, progress_tx.clone())
                    .await?;
            }

            // 2c. Check if any missing non-sync dependencies exist on the AUR
            for dep in missing_other {
                let base_name = dep
                    .split(|c| c == '>' || c == '<' || c == '=' || c == ':')
                    .next()
                    .unwrap_or(&dep)
                    .trim();
                if self.alpm.is_package_installed(base_name) || visited.contains(base_name) {
                    continue;
                }

                if let Ok(Some(_)) = self.client.info(base_name).await {
                    let _ = progress_tx
                        .send(TransactionEvent {
                            step: TransactionStep::CheckingDependencies,
                            progress_fraction: 0.25,
                            log_message: format!("[{}] Installing AUR dependency '{}'...", pkg_name, base_name),
                        })
                        .await;

                    let dep_id = PackageId::new(base_name, PackageSource::Aur);
                    self.install_internal(&dep_id, progress_tx.clone(), visited).await?;
                }
            }

            // 3. Compile package on host (Sandbox disabled by user configuration)
            if cancelled(&pkg_name) {
                self.cancellations.write().remove(&pkg_name);
                return Err(PastorError::Cancelled(format!(
                    "Installation of '{pkg_name}' was cancelled"
                )));
            }

            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::ApplyingChanges {
                        package: pkg_name.clone(),
                        index: 1,
                        total: 1,
                    },
                    progress_fraction: 0.35,
                    log_message: format!("[{}] Building AUR package on host with makepkg...", pkg_name),
                })
                .await;

            let bdir = repo.repo_dir.clone();
            let compiled_pkg_path = tokio::task::spawn_blocking(move || -> Result<std::path::PathBuf, PastorError> {
                // Pre-clean any existing .pkg.tar.* files in the build directory to avoid picking up stale builds
                if let Ok(entries) = std::fs::read_dir(&bdir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.contains(".pkg.tar.") {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }

                // Run makepkg -f --noconfirm (deps are now installed by ALPM worker)
                let output = std::process::Command::new("makepkg")
                    .current_dir(&bdir)
                    .args(["-f", "--noconfirm"])
                    .output()
                    .map_err(|e| PastorError::BackendError {
                        backend: "aur".into(),
                        message: format!("Failed to spawn makepkg: {e}"),
                    })?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let details = if !stderr.trim().is_empty() {
                        stderr.trim().to_string()
                    } else {
                        stdout.trim().to_string()
                    };
                    return Err(PastorError::BackendError {
                        backend: "aur".into(),
                        message: format!("makepkg build failed: {details}"),
                    });
                }

                // Find compiled package artifact, skipping debug packages and picking the newest
                let mut candidates = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&bdir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.contains(".pkg.tar.") && !name.contains("-debug-") {
                            candidates.push(path);
                        }
                    }
                }

                candidates.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
                candidates.into_iter().next_back().ok_or_else(|| PastorError::BackendError {
                    backend: "aur".into(),
                    message: "No compiled .pkg.tar package artifact found after makepkg build".into(),
                })
            })
            .await
            .map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("makepkg build task error: {e}"),
            })??;

            // 4. Pre-Pacman verification of compiled package hooks
            let _ = ArtifactVerifier::verify_package_install_hook(&compiled_pkg_path)?;

            // 5. Install compiled artifact via ALPM privileged worker
            let artifact_name = compiled_pkg_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("artifact");

            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::ApplyingChanges {
                        package: pkg_name.clone(),
                        index: 1,
                        total: 1,
                    },
                    progress_fraction: 0.9,
                    log_message: format!("[{}] Installing verified package {} with ALPM...", pkg_name, artifact_name),
                })
                .await;

            self.alpm
                .install_local_package(&compiled_pkg_path, progress_tx.clone())
                .await?;

            // 6. Record successful install in local state
            let _ = self.state_store.write().record_installed(&pkg_name, &current_commit, maintainer);

            let _ = progress_tx
                .send(TransactionEvent {
                    step: TransactionStep::Completed,
                    progress_fraction: 1.0,
                    log_message: format!("[{}] Successfully installed AUR package", pkg_name),
                })
                .await;

            Ok(())
        })
    }
}

fn extract_dependencies(repo_dir: &std::path::Path) -> Vec<String> {
    let mut deps = Vec::new();

    // 1. Try makepkg --printsrcinfo (evaluated directly from PKGBUILD)
    if let Ok(output) = std::process::Command::new("makepkg")
        .current_dir(repo_dir)
        .arg("--printsrcinfo")
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(dep) = trimmed.strip_prefix("depends = ") {
                    deps.push(dep.trim().to_string());
                } else if let Some(dep) = trimmed.strip_prefix("makedepends = ") {
                    deps.push(dep.trim().to_string());
                } else if let Some(dep) = trimmed.strip_prefix("checkdepends = ") {
                    deps.push(dep.trim().to_string());
                }
            }
        }
    }

    // 2. Fallback: Parse .SRCINFO file if present
    if deps.is_empty() {
        let srcinfo_path = repo_dir.join(".SRCINFO");
        if let Ok(content) = std::fs::read_to_string(&srcinfo_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(dep) = trimmed.strip_prefix("depends = ") {
                    deps.push(dep.trim().to_string());
                } else if let Some(dep) = trimmed.strip_prefix("makedepends = ") {
                    deps.push(dep.trim().to_string());
                } else if let Some(dep) = trimmed.strip_prefix("checkdepends = ") {
                    deps.push(dep.trim().to_string());
                }
            }
        }
    }

    deps.sort();
    deps.dedup();
    deps
}
