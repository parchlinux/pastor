use std::{
    path::PathBuf,
    process::Command,
};
use pastor_core::error::PastorError;

pub struct AurGitRepo {
    pub package_name: String,
    pub repo_dir: PathBuf,
}

pub fn validate_package_name(name: &str) -> Result<(), PastorError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 255 {
        return Err(PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Invalid package name length: '{}'", name),
        });
    }
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '+' || c == '.' || c == '-') {
        return Err(PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Package name '{}' contains illegal characters", name),
        });
    }
    if trimmed.contains("..") || trimmed.starts_with('.') || trimmed.starts_with('-') {
        return Err(PastorError::BackendError {
            backend: "aur".into(),
            message: format!("Package name '{}' contains invalid directory navigation", name),
        });
    }
    Ok(())
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
            let uid = unsafe { libc::getuid() };
            let run_user = format!("/run/user/{}/pastor/aur", uid);
            let run_path = PathBuf::from(&run_user);
            if run_path.parent().map(|p| p.exists()).unwrap_or(false) {
                run_path
            } else {
                PathBuf::from("/var/cache/pastor/aur")
            }
        }
    }

    pub fn for_package(package_name: &str) -> Self {
        let valid_name = if validate_package_name(package_name).is_ok() {
            package_name.trim().to_string()
        } else {
            let clean = std::path::Path::new(package_name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("invalid")
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>();
            if clean.is_empty() {
                "invalid".to_string()
            } else {
                clean
            }
        };

        let mut repo_dir = Self::cache_base_dir();
        repo_dir.push(&valid_name);
        Self {
            package_name: valid_name,
            repo_dir,
        }
    }

    pub fn sync(&self) -> Result<String, PastorError> {
        validate_package_name(&self.package_name)?;

        let repo_url = format!("https://aur.archlinux.org/{}.git", self.package_name);

        if self.repo_dir.join(".git").exists() {
            // Already cloned, perform fetch & fast-forward to FETCH_HEAD
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
                    message: format!("git fetch returned non-zero status for '{}'", self.package_name),
                });
            }

            // QUAL-001 & CONC-004: Always reset to FETCH_HEAD so any default branch works without hardcoding
            let status_reset = Command::new("git")
                .arg("-C")
                .arg(&self.repo_dir)
                .args(["reset", "--hard", "FETCH_HEAD"])
                .status()
                .map_err(|e| PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("Failed to run git reset: {e}"),
                })?;

            if !status_reset.success() {
                return Err(PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("git reset --hard FETCH_HEAD failed for '{}'", self.package_name),
                });
            }
        } else {
            // Fresh clone
            if let Some(parent) = self.repo_dir.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            tracing::info!("Cloning AUR repository for {} from {}", self.package_name, repo_url);
            let target_dir = self.repo_dir.to_string_lossy().to_string();
            let status = Command::new("git")
                .args([
                    "clone",
                    "--depth=50",
                    &repo_url,
                    &target_dir,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_package_name_valid() {
        assert!(validate_package_name("google-chrome").is_ok());
        assert!(validate_package_name("visual-studio-code-bin").is_ok());
        assert!(validate_package_name("parch_theme").is_ok());
        assert!(validate_package_name("lib32-glibc").is_ok());
        assert!(validate_package_name("gcc+test@1.0").is_ok());
    }

    #[test]
    fn test_validate_package_name_invalid_and_traversal() {
        assert!(validate_package_name("").is_err());
        assert!(validate_package_name("../etc/passwd").is_err());
        assert!(validate_package_name("foo/bar").is_err());
        assert!(validate_package_name("foo..bar").is_err());
        assert!(validate_package_name("foo;rm -rf /").is_err());
        assert!(validate_package_name("foo\0bar").is_err());
        assert!(validate_package_name("foo bar").is_err());
    }

    #[test]
    fn test_aur_git_repo_construction_sanitizes_path() {
        let repo = AurGitRepo::for_package("google-chrome");
        assert_eq!(repo.package_name, "google-chrome");
        assert!(repo.repo_dir.ends_with("google-chrome"));

        let repo_invalid = AurGitRepo::for_package("../../sneaky");
        assert_ne!(repo_invalid.package_name, "../../sneaky");
        assert!(!repo_invalid.repo_dir.to_string_lossy().contains(".."));
    }
}
