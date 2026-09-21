use std::{
    path::PathBuf,
    process::Command,
};
use pastor_core::error::PastorError;

pub struct AurGitRepo {
    pub package_name: String,
    pub repo_dir: PathBuf,
}

impl AurGitRepo {
    pub fn cache_base_dir() -> PathBuf {
        if let Ok(cache_home) = std::env::var("XDG_CACHE_HOME") {
            let mut p = PathBuf::from(cache_home);
            p.push("pastor");
            p.push("aur");
            p
        } else if let Ok(home) = std::env::var("HOME") {
            let mut p = PathBuf::from(home);
            p.push(".cache");
            p.push("pastor");
            p.push("aur");
            p
        } else {
            PathBuf::from("/tmp/pastor-aur")
        }
    }

    pub fn for_package(package_name: &str) -> Self {
        let mut repo_dir = Self::cache_base_dir();
        repo_dir.push(package_name);
        Self {
            package_name: package_name.to_string(),
            repo_dir,
        }
    }

    pub fn sync(&self) -> Result<String, PastorError> {
        let repo_url = format!("https://aur.archlinux.org/{}.git", self.package_name);

        if self.repo_dir.join(".git").exists() {
            // Already cloned, perform fetch & fast-forward
            tracing::info!("Pulling latest AUR git commits for {}", self.package_name);
            let status = Command::new("git")
                .arg("-C")
                .arg(&self.repo_dir)
                .args(["fetch", "--depth=50", "origin"])
                .status()
                .map_err(|e| PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("Failed to run git fetch: {e}"),
                })?;

            if !status.success() {
                return Err(PastorError::BackendError {
                    backend: "aur".into(),
                    message: "git fetch returned non-zero status".into(),
                });
            }

            let status_reset = Command::new("git")
                .arg("-C")
                .arg(&self.repo_dir)
                .args(["reset", "--hard", "origin/master"])
                .status();

            if status_reset.is_err() || !status_reset.unwrap().success() {
                // Try main branch if master fails
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&self.repo_dir)
                    .args(["reset", "--hard", "origin/main"])
                    .status();
            }
        } else {
            // Fresh clone
            if let Some(parent) = self.repo_dir.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            tracing::info!("Cloning AUR repository for {} from {}", self.package_name, repo_url);
            let status = Command::new("git")
                .args([
                    "clone",
                    "--depth=50",
                    &repo_url,
                    self.repo_dir.to_str().unwrap_or_default(),
                ])
                .status()
                .map_err(|e| PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("Failed to run git clone: {e}"),
                })?;

            if !status.success() {
                return Err(PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("git clone failed for AUR package '{}'", self.package_name),
                });
            }
        }

        self.current_commit()
    }

    pub fn current_commit(&self) -> Result<String, PastorError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_dir)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Failed to read git commit: {e}"),
            })?;

        if !output.status.success() {
            return Err(PastorError::BackendError {
                backend: "aur".into(),
                message: "git rev-parse HEAD failed".into(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn get_diff(&self, from_commit: Option<&str>) -> Result<String, PastorError> {
        if let Some(base_commit) = from_commit {
            if !base_commit.trim().is_empty() {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&self.repo_dir)
                    .args(["diff", base_commit, "HEAD"])
                    .output()
                    .map_err(|e| PastorError::BackendError {
                        backend: "aur".into(),
                        message: format!("Failed to run git diff: {e}"),
                    })?;

                if output.status.success() {
                    let diff_str = String::from_utf8_lossy(&output.stdout).to_string();
                    if !diff_str.trim().is_empty() {
                        return Ok(diff_str);
                    }
                }
            }
        }

        // If first install or no diff against previous commit, return full PKGBUILD contents
        self.read_pkgbuild()
    }

    pub fn read_pkgbuild(&self) -> Result<String, PastorError> {
        let path = self.repo_dir.join("PKGBUILD");
        std::fs::read_to_string(&path).map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Failed to read PKGBUILD from {}: {e}", path.display()),
        })
    }

    pub fn find_install_files(&self) -> Vec<PathBuf> {
        let mut results = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.repo_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "install" {
                        results.push(path);
                    }
                }
            }
        }
        results
    }
}
