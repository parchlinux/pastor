pub mod inspect;
pub mod manage;

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use inspect::inspect_appimage;
pub use manage::{
    install_appimage, launch_appimage, list_installed_appimages, uninstall_appimage,
    update_appimage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppImageType {
    Type2Squashfs,
    Type2Dwarfs,
    Type1Iso,
    Unknown,
}

impl AppImageType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Type2Squashfs => "Type 2 (SquashFS)",
            Self::Type2Dwarfs => "Type 2 (DwarFS)",
            Self::Type1Iso => "Type 1 (ISO9660)",
            Self::Unknown => "AppImage",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppImageOwner {
    Pastor,
    AppManager,
    External,
}

impl AppImageOwner {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pastor => "Pastor",
            Self::AppManager => "AppManager",
            Self::External => "Local",
        }
    }
}

/// Metadata extracted from an `.AppImage` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppImageMetadata {
    pub name: String,
    pub slug: String,
    pub version: Option<String>,
    pub comment: Option<String>,
    pub categories: Vec<String>,
    pub desktop_filename: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub appimage_type: AppImageType,
    pub file_size_bytes: u64,
    pub is_executable: bool,
}

/// An AppImage installed/integrated on the user's system (by Pastor or AppManager).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledAppImage {
    pub name: String,
    pub version: Option<String>,
    pub comment: Option<String>,
    pub exec_path: PathBuf,
    pub desktop_path: Option<PathBuf>,
    pub icon_path: Option<PathBuf>,
    pub install_dir: Option<PathBuf>,
    pub file_size_bytes: u64,
    pub owner: AppImageOwner,
}

/// Generates normalized search candidate names from an AppImage's metadata
/// to discover Official Repository, Flatpak, or AUR alternatives.
pub fn search_candidates_for_appimage(meta: &AppImageMetadata, filename: Option<&str>) -> Vec<String> {
    let mut queries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push_unique = |s: String| {
        let s = s.trim().to_lowercase();
        if s.len() >= 2 && seen.insert(s.clone()) {
            queries.push(s);
        }
    };

    push_unique(meta.name.clone());
    push_unique(meta.name.replace(' ', "-"));
    push_unique(meta.slug.clone());

    if let Some(df) = &meta.desktop_filename {
        let stem = df.strip_suffix(".desktop").unwrap_or(df);
        push_unique(stem.to_string());
        if let Some(tail) = stem.rsplit('.').next() {
            push_unique(tail.to_string());
        }
    }

    if let Some(fname) = filename {
        let stem = fname
            .strip_suffix(".AppImage")
            .or_else(|| fname.strip_suffix(".appimage"))
            .unwrap_or(fname);
        if let Some(base) = stem.split(['-', '_']).next() {
            push_unique(base.to_string());
        }
    }

    queries
}
