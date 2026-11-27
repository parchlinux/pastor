use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::PastorError;

use super::inspect::{
    detect_appimage_payload, extract_appimage_metadata_files, find_best_icon, inspect_appimage,
    parse_desktop_entry, sanitize_slug,
};
use super::{AppImageOwner, InstalledAppImage};

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn applications_dir() -> PathBuf {
    home_dir().join("Applications")
}

pub fn local_desktop_dir() -> PathBuf {
    home_dir().join(".local/share/applications")
}

pub fn local_icons_dir() -> PathBuf {
    home_dir().join(".local/share/icons/hicolor")
}

fn ensure_executable(path: &Path) -> Result<(), PastorError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| PastorError::InvalidOperation(format!("Cannot stat AppImage: {e}")))?
            .permissions();
        let mode = perms.mode();
        if (mode & 0o111) == 0 {
            perms.set_mode(mode | 0o755);
            fs::set_permissions(path, perms).map_err(|e| {
                PastorError::PermissionDenied(format!("Failed to mark AppImage executable: {e}"))
            })?;
        }
    }
    Ok(())
}

fn extract_exec_binary(exec_val: &str) -> Option<PathBuf> {
    let trimmed = exec_val.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first_token = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next()?
    } else if let Some(rest) = trimmed.strip_prefix('\'') {
        rest.split('\'').next()?
    } else {
        trimmed.split_whitespace().next()?
    };
    if first_token.is_empty() {
        None
    } else {
        Some(PathBuf::from(first_token))
    }
}

fn resolve_icon_path(icon_field: Option<&str>) -> Option<PathBuf> {
    let icon = icon_field?.trim();
    if icon.is_empty() {
        return None;
    }
    let direct = PathBuf::from(icon);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct);
    }
    // Check common local icon locations
    for sub in &["scalable/apps", "256x256/apps", "128x128/apps", "64x64/apps"] {
        for ext in &["", ".svg", ".png"] {
            let p = local_icons_dir().join(sub).join(format!("{icon}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn refresh_desktop_database() {
    let _ = Command::new("update-desktop-database")
        .arg(local_desktop_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Lists all installed/integrated AppImages on the system, including those managed
/// by Pastor, `kem-a/AppManager` (`~/Applications`, `~/Applications/.installed`),
/// or manually placed in `~/Applications`.
pub fn list_installed_appimages() -> Vec<InstalledAppImage> {
    list_installed_appimages_in(&applications_dir(), &local_desktop_dir())
}

pub(crate) fn list_installed_appimages_in(
    apps_dir: &Path,
    desktop_dir: &Path,
) -> Vec<InstalledAppImage> {
    let mut results = Vec::new();
    let mut seen_execs: HashSet<PathBuf> = HashSet::new();
    let appmanager_extracted_root = apps_dir.join(".installed");

    // 1. Scan ~/.local/share/applications/*.desktop for AppImage & AppManager entries
    if let Ok(entries) = fs::read_dir(desktop_dir) {
        for entry in entries.flatten() {
            let dpath = entry.path();
            if dpath.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&dpath) else {
                continue;
            };
            let parsed = parse_desktop_entry(&content);
            let exec_path = parsed
                .try_exec
                .as_deref()
                .and_then(extract_exec_binary)
                .or_else(|| parsed.exec.as_deref().and_then(extract_exec_binary));

            let Some(exec) = exec_path else { continue };
            let exec_str = exec.to_string_lossy().to_lowercase();
            let in_apps_dir = exec.starts_with(apps_dir);
            let is_appimage_ext = exec_str.ends_with(".appimage");

            if !is_appimage_ext && !in_apps_dir && !parsed.appimage_managed {
                continue;
            }
            if !exec.exists() {
                continue;
            }

            let install_dir = if exec.starts_with(&appmanager_extracted_root) {
                exec.strip_prefix(&appmanager_extracted_root)
                    .ok()
                    .and_then(|rel| rel.components().next())
                    .map(|first| appmanager_extracted_root.join(first.as_os_str()))
            } else {
                None
            };

            let file_name = dpath.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let owner = if file_name.starts_with("pastor-appimage-")
                || parsed.raw_fields.get("X-Pastor-Managed").map(|s| s.as_str()) == Some("true")
            {
                AppImageOwner::Pastor
            } else if in_apps_dir || install_dir.is_some() {
                AppImageOwner::AppManager
            } else {
                AppImageOwner::External
            };

            let size = fs::metadata(&exec).map(|m| m.len()).unwrap_or(0);
            let icon_path = resolve_icon_path(parsed.icon.as_deref());
            let name = parsed.name.unwrap_or_else(|| {
                exec.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("AppImage")
                    .to_string()
            });

            seen_execs.insert(exec.clone());
            results.push(InstalledAppImage {
                name,
                version: parsed.version,
                comment: parsed.comment,
                exec_path: exec,
                desktop_path: Some(dpath),
                icon_path,
                install_dir,
                file_size_bytes: size,
                owner,
            });
        }
    }

    // 2. Scan ~/Applications/*.AppImage for standalone AppImages without a .desktop entry
    if let Ok(entries) = fs::read_dir(apps_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let is_appimg = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("appimage"))
                .unwrap_or(false);
            if !is_appimg || seen_execs.contains(&p) {
                continue;
            }
            let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("AppImage")
                .to_string();
            results.push(InstalledAppImage {
                name,
                version: None,
                comment: None,
                exec_path: p,
                desktop_path: None,
                icon_path: None,
                install_dir: None,
                file_size_bytes: size,
                owner: AppImageOwner::External,
            });
        }
    }

    results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    results
}

fn copy_extracted_icon_for(
    appimage_path: &Path,
    slug: &str,
    icons_base: &Path,
) -> Option<PathBuf> {
    let (kind, offset) = detect_appimage_payload(appimage_path);
    let tmp = std::env::temp_dir().join(format!("pastor-icon-{}-{}", slug, std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let ok = extract_appimage_metadata_files(appimage_path, &tmp, kind, offset);
    let found = if ok { find_best_icon(&tmp, None) } else { None };

    let installed_icon = found.and_then(|src| {
        let is_svg = src
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("svg"))
            .unwrap_or(false)
            || fs::read(&src)
                .map(|b| b.windows(4).any(|w| w == b"<svg"))
                .unwrap_or(false);
        let (subdir, ext) = if is_svg {
            ("scalable/apps", "svg")
        } else {
            ("256x256/apps", "png")
        };
        let target_dir = icons_base.join(subdir);
        fs::create_dir_all(&target_dir).ok()?;
        let target = target_dir.join(format!("pastor-appimage-{slug}.{ext}"));
        fs::copy(&src, &target).ok()?;
        Some(target)
    });

    let _ = fs::remove_dir_all(&tmp);
    installed_icon
}

/// Integrates and installs an `.AppImage` into `~/Applications/` and creates a desktop launcher.
pub fn install_appimage(src_path: &Path) -> Result<InstalledAppImage, PastorError> {
    install_appimage_into(
        src_path,
        &applications_dir(),
        &local_desktop_dir(),
        &local_icons_dir(),
    )
}

pub(crate) fn install_appimage_into(
    src_path: &Path,
    apps_dir: &Path,
    desktop_dir: &Path,
    icons_dir: &Path,
) -> Result<InstalledAppImage, PastorError> {
    let meta = inspect_appimage(src_path)?;
    fs::create_dir_all(apps_dir)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed creating ~/Applications: {e}")))?;
    fs::create_dir_all(desktop_dir)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed creating desktop dir: {e}")))?;

    let dest_bin = apps_dir.join(format!("{}.AppImage", meta.slug));
    if src_path != dest_bin {
        fs::copy(src_path, &dest_bin).map_err(|e| {
            PastorError::InvalidOperation(format!("Failed copying AppImage to {}: {e}", dest_bin.display()))
        })?;
    }
    ensure_executable(&dest_bin)?;

    let icon_path = copy_extracted_icon_for(&dest_bin, &meta.slug, icons_dir);
    let icon_value = icon_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "application-x-executable".to_string());

    let desktop_path = desktop_dir.join(format!("pastor-appimage-{}.desktop", meta.slug));
    let cats = if meta.categories.is_empty() {
        "Utility;".to_string()
    } else {
        format!("{};", meta.categories.join(";"))
    };

    let mut desktop_content = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\" %U\nTryExec={}\nIcon={}\nCategories={}\nTerminal=false\nX-Pastor-Managed=true\n",
        meta.name,
        dest_bin.display(),
        dest_bin.display(),
        icon_value,
        cats,
    );
    if let Some(ref c) = meta.comment {
        desktop_content.push_str(&format!("Comment={c}\n"));
    }
    if let Some(ref v) = meta.version {
        desktop_content.push_str(&format!("X-AppImage-Version={v}\n"));
    }

    fs::write(&desktop_path, desktop_content)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed writing .desktop entry: {e}")))?;

    refresh_desktop_database();
    let size = fs::metadata(&dest_bin).map(|m| m.len()).unwrap_or(meta.file_size_bytes);

    Ok(InstalledAppImage {
        name: meta.name,
        version: meta.version,
        comment: meta.comment,
        exec_path: dest_bin,
        desktop_path: Some(desktop_path),
        icon_path,
        install_dir: None,
        file_size_bytes: size,
        owner: AppImageOwner::Pastor,
    })
}

/// Replaces an installed AppImage (including AppManager-installed ones) with a newer `.AppImage` file.
pub fn update_appimage(
    existing: &InstalledAppImage,
    new_src: &Path,
) -> Result<InstalledAppImage, PastorError> {
    // If existing was an extracted AppManager install dir, clean it up and install cleanly
    if existing.install_dir.is_some() {
        uninstall_appimage(existing)?;
        return install_appimage(new_src);
    }

    let meta = inspect_appimage(new_src)?;
    if new_src != existing.exec_path {
        fs::copy(new_src, &existing.exec_path).map_err(|e| {
            PastorError::InvalidOperation(format!("Failed updating AppImage binary: {e}"))
        })?;
    }
    ensure_executable(&existing.exec_path)?;

    let slug = sanitize_slug(&existing.name);
    let new_icon = copy_extracted_icon_for(&existing.exec_path, &slug, &local_icons_dir());

    if let Some(ref dpath) = existing.desktop_path {
        if let Ok(old_content) = fs::read_to_string(dpath) {
            let mut updated_lines: Vec<String> = old_content
                .lines()
                .filter(|l| !l.starts_with("X-AppImage-Version="))
                .map(|l| l.to_string())
                .collect();
            if let Some(ref v) = meta.version {
                updated_lines.push(format!("X-AppImage-Version={v}"));
            }
            let _ = fs::write(dpath, updated_lines.join("\n") + "\n");
        }
    }

    refresh_desktop_database();
    let size = fs::metadata(&existing.exec_path)
        .map(|m| m.len())
        .unwrap_or(meta.file_size_bytes);

    Ok(InstalledAppImage {
        name: existing.name.clone(),
        version: meta.version.or_else(|| existing.version.clone()),
        comment: meta.comment.or_else(|| existing.comment.clone()),
        exec_path: existing.exec_path.clone(),
        desktop_path: existing.desktop_path.clone(),
        icon_path: new_icon.or_else(|| existing.icon_path.clone()),
        install_dir: None,
        file_size_bytes: size,
        owner: existing.owner,
    })
}

/// Uninstalls an AppImage (supporting both Pastor-managed and `AppManager`-installed apps).
pub fn uninstall_appimage(installed: &InstalledAppImage) -> Result<(), PastorError> {
    if let Some(ref dpath) = installed.desktop_path {
        if dpath.is_file() {
            fs::remove_file(dpath).map_err(|e| {
                PastorError::InvalidOperation(format!("Failed removing desktop file: {e}"))
            })?;
        }
    }

    if let Some(ref inst_dir) = installed.install_dir {
        if inst_dir.is_dir() {
            fs::remove_dir_all(inst_dir).map_err(|e| {
                PastorError::InvalidOperation(format!("Failed removing AppManager install dir: {e}"))
            })?;
        }
    } else if installed.exec_path.is_file() {
        fs::remove_file(&installed.exec_path).map_err(|e| {
            PastorError::InvalidOperation(format!("Failed removing AppImage binary: {e}"))
        })?;
    }

    if let Some(ref ipath) = installed.icon_path {
        let local_icons = local_icons_dir();
        let apps = applications_dir();
        if (ipath.starts_with(&local_icons) || ipath.starts_with(&apps)) && ipath.is_file() {
            let _ = fs::remove_file(ipath);
        }
    }

    refresh_desktop_database();
    Ok(())
}

/// Marks an AppImage executable and launches it detached.
pub fn launch_appimage(path: &Path) -> Result<(), PastorError> {
    if !path.is_file() {
        return Err(PastorError::PackageNotFound(format!(
            "AppImage executable not found: {}",
            path.display()
        )));
    }
    ensure_executable(path)?;
    Command::new(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed launching AppImage: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_list_and_uninstall_interoperability() {
        let base = std::env::temp_dir().join(format!("pastor-appimg-mgr-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let apps = base.join("Applications");
        let desktop = base.join("desktop");
        let icons = base.join("icons");
        fs::create_dir_all(&apps).unwrap();
        fs::create_dir_all(&desktop).unwrap();

        // Simulate an AppManager-installed extracted AppImage in ~/Applications/.installed/demo/
        let appmgr_dir = apps.join(".installed/demo");
        fs::create_dir_all(&appmgr_dir).unwrap();
        let appmgr_exec = appmgr_dir.join("AppRun");
        fs::write(&appmgr_exec, b"#!/bin/sh\necho hi\n").unwrap();
        let appmgr_desktop = desktop.join("demo-appmanager.desktop");
        fs::write(
            &appmgr_desktop,
            format!(
                "[Desktop Entry]\nType=Application\nName=Demo AppManager App\nExec=\"{}\" %U\nX-AppImage-Version=2.1.0\n",
                appmgr_exec.display()
            ),
        )
        .unwrap();

        // Simulate a Pastor-installed AppImage
        let src = base.join("SampleApp-1.0.AppImage");
        fs::write(&src, b"#!/bin/sh\necho sample\n").unwrap();
        let installed_pastor = install_appimage_into(&src, &apps, &desktop, &icons).unwrap();
        assert_eq!(installed_pastor.owner, AppImageOwner::Pastor);

        // List should discover BOTH Pastor and AppManager entries
        let list = list_installed_appimages_in(&apps, &desktop);
        assert_eq!(list.len(), 2);
        let appmgr_item = list.iter().find(|i| i.name == "Demo AppManager App").unwrap();
        assert_eq!(appmgr_item.owner, AppImageOwner::AppManager);
        assert_eq!(appmgr_item.install_dir.as_deref(), Some(appmgr_dir.as_path()));

        // Uninstall AppManager item cleanly
        uninstall_appimage(appmgr_item).unwrap();
        assert!(!appmgr_dir.exists());
        assert!(!appmgr_desktop.exists());

        // Uninstall Pastor item cleanly
        uninstall_appimage(&installed_pastor).unwrap();
        assert!(!installed_pastor.exec_path.exists());

        let _ = fs::remove_dir_all(&base);
    }
}
