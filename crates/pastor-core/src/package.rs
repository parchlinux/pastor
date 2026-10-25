use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParchRepoType {
    /// Official Parch Linux stable software repository
    World,
    /// Official Parch Linux unstable / bleeding-edge software repository
    Void,
}

impl std::fmt::Display for ParchRepoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::World => write!(f, "world"),
            Self::Void => write!(f, "void"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageSource {
    /// Parch Linux official repository ([world] or [void])
    Parch(ParchRepoType),
    /// Arch Linux upstream repository (core, extra, multilib)
    Arch(String),
    /// Arch User Repository
    Aur,
    /// Flatpak application/runtime
    Flatpak { remote: String },
    /// bootc base OS image / package
    Bootc,
    /// Waydroid Android application
    Waydroid,
}

impl PackageSource {
    pub fn display_badge(&self) -> &'static str {
        match self {
            Self::Parch(ParchRepoType::World) => "Parch [world]",
            Self::Parch(ParchRepoType::Void) => "Parch [void]",
            Self::Arch(_) => "Arch Repo",
            Self::Aur => "AUR",
            Self::Flatpak { .. } => "Flatpak",
            Self::Bootc => "bootc (OS)",
            Self::Waydroid => "Android (Waydroid)",
        }
    }

    pub fn is_parch(&self) -> bool {
        matches!(self, Self::Parch(_))
    }

    pub fn is_parch_world(&self) -> bool {
        matches!(self, Self::Parch(ParchRepoType::World))
    }

    pub fn is_parch_void(&self) -> bool {
        matches!(self, Self::Parch(ParchRepoType::Void))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub source: PackageSource,
}

impl PackageId {
    pub fn new(name: impl Into<String>, source: PackageSource) -> Self {
        Self {
            name: name.into(),
            source,
        }
    }

    /// Stable string key combining source and name, used for matching
    /// transaction events to UI rows even when callers pass loosely-typed ids.
    pub fn event_key(&self) -> String {
        let src = match &self.source {
            PackageSource::Parch(ParchRepoType::World) => "parch-world",
            PackageSource::Parch(ParchRepoType::Void) => "parch-void",
            PackageSource::Arch(_) => "arch",
            PackageSource::Aur => "aur",
            PackageSource::Flatpak { remote } => return format!("flatpak:{remote}:{}", self.name),
            PackageSource::Bootc => "bootc",
            PackageSource::Waydroid => "waydroid",
        };
        format!("{src}:{}", self.name)
    }

    /// Package name with a trailing `.desktop` suffix removed and lowercased.
    pub fn clean_name(&self) -> String {
        let n = self.name.strip_suffix(".desktop").unwrap_or(&self.name);
        n.to_ascii_lowercase()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageCategory {
    Featured,
    ParchPicks,
    Development,
    Games,
    Graphics,
    Internet,
    Multimedia,
    Office,
    System,
    Android,
    Utilities,
}

impl PackageCategory {
    pub fn all() -> &'static [PackageCategory] {
        &[
            Self::Development,
            Self::Games,
            Self::Graphics,
            Self::Internet,
            Self::Multimedia,
            Self::Office,
            Self::System,
            Self::Android,
            Self::Utilities,
        ]
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Featured => "Featured",
            Self::ParchPicks => "Parch Picks",
            Self::Development => "Development",
            Self::Games => "Games",
            Self::Graphics => "Graphics & Design",
            Self::Internet => "Internet & Web",
            Self::Multimedia => "Audio & Video",
            Self::Office => "Office & Productivity",
            Self::System => "System Tools",
            Self::Android => "Android Apps (Waydroid)",
            Self::Utilities => "Utilities",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Featured => "emblem-favorite",
            Self::ParchPicks => "starred",
            Self::Development => "applications-development",
            Self::Games => "applications-games",
            Self::Graphics => "applications-graphics",
            Self::Internet => "applications-internet",
            Self::Multimedia => "applications-multimedia",
            Self::Office => "applications-office",
            Self::System => "applications-system",
            Self::Android => "phone",
            Self::Utilities => "applications-utilities",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageState {
    NotInstalled,
    Installed,
    UpdateAvailable,
    Processing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackageIcon {
    Themed(String),
    LocalPath(String),
    RemoteUrl(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub id: PackageId,
    pub name: String,
    pub display_name: Option<String>,
    pub version: String,
    pub installed_version: Option<String>,
    pub summary: String,
    pub description: Option<String>,
    pub icon: Option<PackageIcon>,
    pub screenshots: Vec<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub maintainer: Option<String>,
    pub categories: Vec<PackageCategory>,
    pub size_installed: Option<u64>,
    pub size_download: Option<u64>,
    pub dependencies: Vec<String>,
    pub changelog: Option<String>,
    pub state: PackageState,
}

impl Package {
    pub fn display_title(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    pub fn is_installed(&self) -> bool {
        matches!(self.state, PackageState::Installed | PackageState::UpdateAvailable)
    }

    pub fn formatted_installed_size(&self) -> Option<String> {
        self.size_installed.map(format_size)
    }

    pub fn formatted_download_size(&self) -> Option<String> {
        self.size_download.map(format_size)
    }
}

/// Format byte counts dynamically into human-readable strings (B, KB, MB, GB) based on size.
pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageUpdate {
    pub id: PackageId,
    pub current_version: String,
    pub new_version: String,
    pub download_size: Option<u64>,
    pub changelog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    /// ALPM Core (Native Arch Linux & Parch Linux packages)
    pub enable_alpm: bool,
    /// Flatpak Core (Sandboxed Flathub applications)
    pub enable_flatpak: bool,

    pub enable_parch_world: bool,
    pub enable_parch_void: bool,
    pub enable_arch: bool,
    pub enable_aur: bool,
    #[serde(default)]
    pub disabled_alpm_repos: Vec<String>,
    pub enable_bootc: bool,
    pub enable_waydroid: bool,
    pub enable_snapper: bool,

    pub check_updates_interval: u32,
    pub notify_updates: bool,
    pub notify_finish: bool,
    pub parallel_downloads: u32,
    pub display_badges: bool,
    pub retained_snapshots: u32,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            enable_alpm: true,
            enable_flatpak: true,
            enable_parch_world: true,
            enable_parch_void: true,
            enable_arch: true,
            enable_aur: true,
            disabled_alpm_repos: Vec::new(),
            enable_bootc: false,
            enable_waydroid: false,
            enable_snapper: true,
            check_updates_interval: 0,
            notify_updates: true,
            notify_finish: true,
            parallel_downloads: 5,
            display_badges: true,
            retained_snapshots: 20,
        }
    }
}

impl ModuleConfig {
    pub fn is_repo_enabled(&self, repo: &str) -> bool {
        let lower = repo.to_ascii_lowercase();
        if lower == "world" && !self.enable_parch_world {
            return false;
        }
        if lower == "void" && !self.enable_parch_void {
            return false;
        }
        if (lower == "core" || lower == "extra" || lower == "multilib") && !self.enable_arch {
            return false;
        }
        !self.disabled_alpm_repos.iter().any(|r| r.eq_ignore_ascii_case(repo))
    }

    pub fn set_repo_enabled(&mut self, repo: &str, enabled: bool) {
        let lower = repo.to_ascii_lowercase();
        if enabled {
            self.disabled_alpm_repos.retain(|r| !r.eq_ignore_ascii_case(repo));
            if lower == "world" {
                self.enable_parch_world = true;
            } else if lower == "void" {
                self.enable_parch_void = true;
            } else if lower == "core" || lower == "extra" || lower == "multilib" {
                self.enable_arch = true;
            }
        } else {
            if !self.disabled_alpm_repos.iter().any(|r| r.eq_ignore_ascii_case(repo)) {
                self.disabled_alpm_repos.push(repo.to_string());
            }
            if lower == "world" {
                self.enable_parch_world = false;
            } else if lower == "void" {
                self.enable_parch_void = false;
            } else if lower == "core" || lower == "extra" || lower == "multilib" {
                let arch_repos = ["core", "extra", "multilib"];
                if arch_repos.iter().all(|r| self.disabled_alpm_repos.iter().any(|d| d.eq_ignore_ascii_case(r))) {
                    self.enable_arch = false;
                }
            }
        }
    }

    pub fn is_source_enabled(&self, source: &PackageSource) -> bool {
        match source {
            PackageSource::Parch(ParchRepoType::World) => self.is_repo_enabled("world"),
            PackageSource::Parch(ParchRepoType::Void) => self.is_repo_enabled("void"),
            PackageSource::Arch(repo_name) => self.is_repo_enabled(repo_name),
            PackageSource::Aur => self.enable_aur,
            PackageSource::Flatpak { .. } => self.enable_flatpak,
            PackageSource::Bootc => self.enable_bootc,
            PackageSource::Waydroid => self.enable_waydroid,
        }
    }

    pub fn update_check_interval(&self) -> Option<std::time::Duration> {
        match self.check_updates_interval {
            0 => Some(std::time::Duration::from_secs(86400)), // Daily
            1 => Some(std::time::Duration::from_secs(21600)), // 6 Hours
            2 => Some(std::time::Duration::from_secs(604800)), // Weekly
            _ => None, // Manual only
        }
    }

    pub fn config_path() -> std::path::PathBuf {
        if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
            let mut p = std::path::PathBuf::from(config_home);
            p.push("pastor");
            p.push("config.json");
            p
        } else if let Ok(home) = std::env::var("HOME") {
            let mut p = std::path::PathBuf::from(home);
            p.push(".config");
            p.push("pastor");
            p.push("config.json");
            p
        } else {
            let uid = unsafe { libc::getuid() };
            let run_user = format!("/run/user/{}/pastor/config.json", uid);
            let run_path = std::path::PathBuf::from(&run_user);
            if run_path.parent().map(|p| p.exists()).unwrap_or(false) {
                run_path
            } else {
                std::path::PathBuf::from("/etc/pastor/config.json")
            }
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.is_file() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Self>(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        tracing::warn!("Failed to parse config.json, backing up corrupted file: {e}");
                        let backup = path.with_extension(format!("corrupted.{}.bak", chrono::Utc::now().timestamp()));
                        let _ = std::fs::copy(&path, &backup);
                    }
                },
                Err(_) => {}
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        // CONC-002: Atomic write via temporary file and rename
        let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(tmp_path, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_config_repo_enabled_roundtrip() {
        let mut cfg = ModuleConfig::default();
        assert!(cfg.is_repo_enabled("world"));
        assert!(cfg.is_repo_enabled("void"));
        assert!(cfg.is_repo_enabled("core"));
        assert!(cfg.is_repo_enabled("extra"));
        assert!(cfg.is_repo_enabled("multilib"));

        // Disable world
        cfg.set_repo_enabled("world", false);
        assert!(!cfg.is_repo_enabled("world"));
        assert!(!cfg.enable_parch_world);

        // Re-enable world
        cfg.set_repo_enabled("world", true);
        assert!(cfg.is_repo_enabled("world"));
        assert!(cfg.enable_parch_world);

        // Disable individual Arch repo
        cfg.set_repo_enabled("core", false);
        assert!(!cfg.is_repo_enabled("core"));
        assert!(cfg.is_repo_enabled("extra"));
        assert!(cfg.enable_arch);

        // Disable remaining Arch repos
        cfg.set_repo_enabled("extra", false);
        cfg.set_repo_enabled("multilib", false);
        assert!(!cfg.is_repo_enabled("core"));
        assert!(!cfg.is_repo_enabled("extra"));
        assert!(!cfg.is_repo_enabled("multilib"));
        assert!(!cfg.enable_arch);

        // Re-enabling one arch repo brings enable_arch back to true
        cfg.set_repo_enabled("core", true);
        assert!(cfg.is_repo_enabled("core"));
        assert!(cfg.enable_arch);
        assert!(!cfg.is_repo_enabled("extra"));
    }
}

