use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AurPackageRecord {
    pub name: String,
    pub last_installed_commit: String,
    pub last_maintainer: Option<String>,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AurStateStore {
    pub packages: HashMap<String, AurPackageRecord>,
}

impl AurStateStore {
    pub fn default_path() -> PathBuf {
        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            let mut p = PathBuf::from(data_home);
            p.push("pastor");
            p.push("aur_state.json");
            p
        } else if let Ok(home) = std::env::var("HOME") {
            let mut p = PathBuf::from(home);
            p.push(".local");
            p.push("share");
            p.push("pastor");
            p.push("aur_state.json");
            p
        } else {
            let uid = unsafe { libc::getuid() };
            let run_user = format!("/run/user/{}/pastor/aur_state.json", uid);
            let run_path = PathBuf::from(&run_user);
            if run_path.parent().map(|p| p.exists()).unwrap_or(false) {
                run_path
            } else {
                PathBuf::from("/var/lib/pastor/aur_state.json")
            }
        }
    }

    pub fn load() -> Self {
        let path = Self::default_path();
        if path.is_file() {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Self>(&content) {
                    Ok(store) => return store,
                    Err(e) => {
                        tracing::warn!("Failed to parse AUR state at {:?}: {e}; saving backup before reset", path);
                        let backup = path.with_extension(format!("corrupted.{}.bak", chrono::Utc::now().timestamp()));
                        let _ = fs::copy(&path, &backup);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read AUR state at {:?}: {e}", path);
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::default_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        // CONC-002: Atomic write via temporary file and rename
        let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        fs::write(&tmp_path, json)?;
        fs::rename(tmp_path, path)
    }

    pub fn get_record(&self, pkg_name: &str) -> Option<&AurPackageRecord> {
        self.packages.get(pkg_name)
    }

    pub fn record_installed(
        &mut self,
        pkg_name: &str,
        commit: &str,
        maintainer: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let record = AurPackageRecord {
            name: pkg_name.to_string(),
            last_installed_commit: commit.to_string(),
            last_maintainer: maintainer.map(|s| s.to_string()),
            installed_at: chrono::Utc::now().to_rfc3339(),
        };
        self.packages.insert(pkg_name.to_string(), record);
        self.save()
    }

    pub fn remove_record(&mut self, pkg_name: &str) -> Result<(), std::io::Error> {
        self.packages.remove(pkg_name);
        self.save()
    }
}
