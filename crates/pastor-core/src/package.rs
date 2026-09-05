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
    pub enable_parch_world: bool,
    pub enable_parch_void: bool,
    pub enable_arch: bool,
    pub enable_aur: bool,
    pub enable_flatpak: bool,
    pub enable_bootc: bool,
    pub enable_waydroid: bool,
    pub enable_snapper: bool,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            enable_parch_world: true,
            enable_parch_void: true,
            enable_arch: true,
            enable_aur: true,
            enable_flatpak: true,
            enable_bootc: false,
            enable_waydroid: false,
            enable_snapper: true,
        }
    }
}

impl ModuleConfig {
    pub fn is_source_enabled(&self, source: &PackageSource) -> bool {
        match source {
            PackageSource::Parch(ParchRepoType::World) => self.enable_parch_world,
            PackageSource::Parch(ParchRepoType::Void) => self.enable_parch_void,
            PackageSource::Arch(_) => self.enable_arch,
            PackageSource::Aur => self.enable_aur,
            PackageSource::Flatpak { .. } => self.enable_flatpak,
            PackageSource::Bootc => self.enable_bootc,
            PackageSource::Waydroid => self.enable_waydroid,
        }
    }
}

