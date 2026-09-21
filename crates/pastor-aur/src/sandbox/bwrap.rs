use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use pastor_core::{
    error::PastorError,
    transaction::{TransactionEvent, TransactionStep},
};
use tokio::sync::mpsc::Sender;

pub struct BwrapSandbox;

impl BwrapSandbox {
    pub fn is_available() -> bool {
        Command::new("bwrap")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Configure common bwrap isolation arguments with network and DNS access
    fn apply_common_args(cmd: &mut Command, bdir: &Path) {
        cmd.args([
            "--unshare-user",
            "--unshare-pid",
            "--unshare-uts",
            // Note: network namespace is NOT unshared so source compilation (cargo, go, git, npm) can fetch dependencies
            "--ro-bind", "/usr", "/usr",
            "--symlink", "usr/bin", "/bin",
            "--symlink", "usr/lib", "/lib",
            "--symlink", "usr/lib", "/lib64",
            "--ro-bind", "/etc", "/etc",
            "--ro-bind-try", "/run/systemd/resolve", "/run/systemd/resolve",
            "--ro-bind-try", "/run/NetworkManager", "/run/NetworkManager",
            "--ro-bind-try", "/run/resolvconf", "/run/resolvconf",
            "--proc", "/proc",
            "--dev", "/dev",
            "--tmpfs", "/tmp",
            "--bind", bdir.to_str().unwrap(), bdir.to_str().unwrap(),
            "--chdir", bdir.to_str().unwrap(),
            "--setenv", "PATH", "/usr/bin:/bin",
            "--setenv", "HOME", bdir.to_str().unwrap(),
        ]);

        for (k, v) in std::env::vars() {
            let upper = k.to_uppercase();
            if upper.contains("PROXY") || upper.contains("SSL") || upper == "LANG" || upper == "LC_ALL" {
                cmd.args(["--setenv", &k, &v]);
            }
        }

        cmd.arg("--die-with-parent");
    }

    /// Phase A: Fetch and verify sources inside sandbox with network enabled
    pub async fn run_phase_a_fetch(
        build_dir: &Path,
        tx: Sender<TransactionEvent>,
    ) -> Result<(), PastorError> {
        let _ = tx
            .send(TransactionEvent {
                step: TransactionStep::Downloading {
                    current_bytes: 0,
                    total_bytes: 0,
                    speed_bps: 0,
                },
                progress_fraction: 0.2,
                log_message: "Sandbox Phase A: Fetching and verifying upstream sources...".to_string(),
            })
            .await;

        let bdir = build_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new("bwrap");
            Self::apply_common_args(&mut cmd, &bdir);
            cmd.args([
                "makepkg",
                "--nobuild",
                "--noconfirm",
                "--nodeps",
            ]);

            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            let mut child = cmd.spawn().map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Failed to spawn bwrap Phase A fetch: {e}"),
            })?;

            let status = child.wait().map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Error waiting for bwrap Phase A: {e}"),
            })?;

            if !status.success() {
                let mut err_msg = String::new();
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_string(&mut err_msg);
                }
                return Err(PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("makepkg Phase A source fetch failed: {err_msg}"),
                });
            }

            Ok(())
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Phase A task error: {e}"),
        })?
    }

    /// Phase B: Compile and package inside filesystem-isolated sandbox with network enabled
    pub async fn run_phase_b_build(
        build_dir: &Path,
        tx: Sender<TransactionEvent>,
    ) -> Result<PathBuf, PastorError> {
        let _ = tx
            .send(TransactionEvent {
                step: TransactionStep::ApplyingChanges {
                    package: build_dir.file_name().and_then(|f| f.to_str()).unwrap_or("AUR package").to_string(),
                    index: 1,
                    total: 1,
                },
                progress_fraction: 0.5,
                log_message: "Sandbox Phase B: Compiling & packaging in sandbox (network enabled for source dependencies)...".to_string(),
            })
            .await;

        let bdir = build_dir.to_path_buf();
        let compiled_pkg = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new("bwrap");
            Self::apply_common_args(&mut cmd, &bdir);
            cmd.args([
                "makepkg",
                "--noextract",
                "--nocheck",
                "--noconfirm",
                "--nodeps",
            ]);

            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            let mut child = cmd.spawn().map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Failed to spawn bwrap Phase B build: {e}"),
            })?;

            let status = child.wait().map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Error waiting for bwrap Phase B: {e}"),
            })?;

            if !status.success() {
                let mut err_msg = String::new();
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_string(&mut err_msg);
                }
                return Err(PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("makepkg Phase B compilation failed: {err_msg}"),
                });
            }

            // Find compiled .pkg.tar.zst
            let mut found_pkg = None;
            if let Ok(entries) = std::fs::read_dir(&bdir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name.contains(".pkg.tar.") {
                        found_pkg = Some(path);
                        break;
                    }
                }
            }

            found_pkg.ok_or_else(|| PastorError::BackendError {
                backend: "aur".into(),
                message: "No compiled .pkg.tar artifact found after sandboxed makepkg completed".into(),
            })
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Phase B task error: {e}"),
        })??;

        let _ = tx
            .send(TransactionEvent {
                step: TransactionStep::CheckingIntegrity,
                progress_fraction: 0.8,
                log_message: format!(
                    "Sandbox build successful: {}",
                    compiled_pkg.file_name().and_then(|n| n.to_str()).unwrap_or("package")
                ),
            })
            .await;

        Ok(compiled_pkg)
    }
}
