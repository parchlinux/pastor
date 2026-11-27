use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::cache::pastor_cache_dir;
use crate::error::PastorError;

use super::{AppImageMetadata, AppImageType};

#[derive(Debug, Clone, Default)]
pub struct DesktopEntryData {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub version: Option<String>,
    pub categories: Vec<String>,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub appimage_managed: bool,
    pub raw_fields: HashMap<String, String>,
}

pub fn sanitize_slug(input: &str) -> String {
    let slug: String = input
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let collapsed: String = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        "appimage-app".to_string()
    } else {
        collapsed
    }
}

/// Parses `[Desktop Entry]` key-value pairs from a `.desktop` file string.
pub fn parse_desktop_entry(content: &str) -> DesktopEntryData {
    let mut data = DesktopEntryData::default();
    let mut in_desktop_entry = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim().to_string();
            data.raw_fields.insert(key.to_string(), val.clone());
            match key {
                "Name" if data.name.is_none() => data.name = Some(val),
                "Comment" if data.comment.is_none() => data.comment = Some(val),
                "GenericName" if data.comment.is_none() => data.comment = Some(val),
                "Icon" if data.icon.is_none() => data.icon = Some(val),
                "Exec" if data.exec.is_none() => data.exec = Some(val),
                "TryExec" if data.try_exec.is_none() => data.try_exec = Some(val),
                "X-AppImage-Version" => data.version = Some(val),
                "Version" if data.version.is_none() && val != "1.0" && val != "1.5" => {
                    data.version = Some(val);
                }
                "Categories" => {
                    data.categories = val
                        .split(';')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "X-Pastor-Managed" | "X-AppImage-Integrate" => {
                    if val == "true" {
                        data.appimage_managed = true;
                    }
                }
                _ => {}
            }
        }
    }
    data
}

/// Computes the end of the ELF image from ELF64/ELF32 header fields (`e_shoff + e_shnum * e_shentsize`).
pub fn find_elf_end_offset(file: &mut File) -> Option<u64> {
    let mut hdr = [0u8; 64];
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_exact(&mut hdr).ok()?;
    if &hdr[0..4] != b"\x7fELF" {
        return None;
    }

    let class = hdr[4];
    let is_le = hdr[5] == 1;
    let read_u16 = |b: &[u8]| -> u16 {
        if is_le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        }
    };
    let read_u32 = |b: &[u8]| -> u32 {
        if is_le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
    };
    let read_u64 = |b: &[u8]| -> u64 {
        if is_le {
            u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        } else {
            u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        }
    };

    if class == 2 {
        // ELF64: e_shoff @ 40 (8B), e_shentsize @ 58 (2B), e_shnum @ 60 (2B)
        let e_shoff = read_u64(&hdr[40..48]);
        let e_shentsize = read_u16(&hdr[58..60]) as u64;
        let e_shnum = read_u16(&hdr[60..62]) as u64;
        if e_shoff > 0 && e_shentsize > 0 && e_shnum > 0 {
            return Some(e_shoff + e_shentsize * e_shnum);
        }
    } else if class == 1 {
        // ELF32: e_shoff @ 32 (4B), e_shentsize @ 46 (2B), e_shnum @ 48 (2B)
        let e_shoff = read_u32(&hdr[32..36]) as u64;
        let e_shentsize = read_u16(&hdr[46..48]) as u64;
        let e_shnum = read_u16(&hdr[48..50]) as u64;
        if e_shoff > 0 && e_shentsize > 0 && e_shnum > 0 {
            return Some(e_shoff + e_shentsize * e_shnum);
        }
    }
    None
}

/// Detects the embedded filesystem type and byte offset in an `.AppImage`.
pub fn detect_appimage_payload(path: &Path) -> (AppImageType, Option<u64>) {
    let Ok(mut file) = File::open(path) else {
        return (AppImageType::Unknown, None);
    };

    if let Some(offset) = find_elf_end_offset(&mut file) {
        let mut magic = [0u8; 8];
        if file.seek(SeekFrom::Start(offset)).is_ok() && file.read_exact(&mut magic).is_ok() {
            if &magic[0..4] == b"hsqs" {
                return (AppImageType::Type2Squashfs, Some(offset));
            }
            if &magic[0..6] == b"DWARFS" {
                return (AppImageType::Type2Dwarfs, Some(offset));
            }
        }
    }

    // Check for Type 1 ISO9660 primary volume descriptor at 0x8001
    let mut iso_magic = [0u8; 5];
    if file.seek(SeekFrom::Start(0x8001)).is_ok() && file.read_exact(&mut iso_magic).is_ok() {
        if &iso_magic == b"CD001" {
            return (AppImageType::Type1Iso, None);
        }
    }

    // Scan first 1 MB for `hsqs` fallback
    let mut buf = vec![0u8; 1024 * 1024];
    if file.seek(SeekFrom::Start(0)).is_ok() {
        if let Ok(n) = file.read(&mut buf) {
            if let Some(pos) = buf[..n].windows(4).position(|w| w == b"hsqs") {
                return (AppImageType::Type2Squashfs, Some(pos as u64));
            }
            if let Some(pos) = buf[..n].windows(6).position(|w| w == b"DWARFS") {
                return (AppImageType::Type2Dwarfs, Some(pos as u64));
            }
        }
    }

    (AppImageType::Unknown, None)
}

/// Extracts root `.desktop` and icon files from an `.AppImage` into `dest_dir` safely.
pub(crate) fn extract_appimage_metadata_files(
    appimage_path: &Path,
    dest_dir: &Path,
    appimage_type: AppImageType,
    offset: Option<u64>,
) -> bool {
    let _ = fs::create_dir_all(dest_dir);

    match (appimage_type, offset) {
        (AppImageType::Type2Squashfs, Some(off)) => {
            let ok = Command::new("unsquashfs")
                .args([
                    "-q",
                    "-f",
                    "-n",
                    "-o",
                    &off.to_string(),
                    "-d",
                    &dest_dir.to_string_lossy(),
                    &appimage_path.to_string_lossy(),
                    "*.desktop",
                    ".DirIcon",
                    "*.png",
                    "*.svg",
                    "usr/share/icons/*",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
        (AppImageType::Type2Dwarfs, Some(off)) => {
            let ok = Command::new("dwarfsextract")
                .args([
                    "-i",
                    &appimage_path.to_string_lossy(),
                    "-O",
                    &off.to_string(),
                    "-o",
                    &dest_dir.to_string_lossy(),
                    "--pattern=*.desktop",
                    "--pattern=.DirIcon",
                    "--pattern=*.png",
                    "--pattern=*.svg",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
        (AppImageType::Type1Iso, _) => {
            let ok = Command::new("bsdtar")
                .args([
                    "-xf",
                    &appimage_path.to_string_lossy(),
                    "-C",
                    &dest_dir.to_string_lossy(),
                    "--include=*.desktop",
                    "--include=.DirIcon",
                    "--include=*.png",
                    "--include=*.svg",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
        _ => {}
    }

    false
}

fn find_first_desktop_file(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("desktop") {
            return Some(p);
        }
    }
    None
}

pub(crate) fn find_best_icon(dir: &Path, icon_name: Option<&str>) -> Option<PathBuf> {
    let dir_icon = dir.join(".DirIcon");
    if dir_icon.is_file() {
        return Some(dir_icon);
    }

    if let Some(name) = icon_name {
        for ext in &["svg", "png"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // Walk extracted directory for any svg/png
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(rd) = fs::read_dir(&cur) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("svg") | Some("png")
            ) {
                return Some(p);
            }
        }
    }
    None
}

/// Inspects an `.AppImage` file safely without executing it.
pub fn inspect_appimage(path: &Path) -> Result<AppImageMetadata, PastorError> {
    if !path.is_file() {
        return Err(PastorError::PackageNotFound(format!(
            "AppImage file not found: {}",
            path.display()
        )));
    }

    let meta_fs = fs::metadata(path)
        .map_err(|e| PastorError::InvalidOperation(format!("Cannot stat AppImage: {e}")))?;
    let file_size_bytes = meta_fs.len();

    #[cfg(unix)]
    let is_executable = {
        use std::os::unix::fs::PermissionsExt;
        (meta_fs.permissions().mode() & 0o111) != 0
    };
    #[cfg(not(unix))]
    let is_executable = true;

    let (appimage_type, offset) = detect_appimage_payload(path);

    let fallback_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("AppImage")
        .split(['-', '_'])
        .next()
        .unwrap_or("AppImage")
        .to_string();

    let preview_dir = pastor_cache_dir()
        .join("appimage-preview")
        .join(sanitize_slug(
            path.file_name().and_then(|s| s.to_str()).unwrap_or("app"),
        ));
    let _ = fs::remove_dir_all(&preview_dir);

    let extracted = extract_appimage_metadata_files(path, &preview_dir, appimage_type, offset);

    let mut desktop_filename = None;
    let mut entry_data = DesktopEntryData::default();

    if extracted {
        if let Some(df) = find_first_desktop_file(&preview_dir) {
            desktop_filename = df.file_name().map(|s| s.to_string_lossy().to_string());
            if let Ok(text) = fs::read_to_string(&df) {
                entry_data = parse_desktop_entry(&text);
            }
        }
    }

    let name = entry_data.name.unwrap_or(fallback_name);
    let slug = sanitize_slug(&name);
    let icon_path = if extracted {
        find_best_icon(&preview_dir, entry_data.icon.as_deref())
    } else {
        None
    };

    Ok(AppImageMetadata {
        name,
        slug,
        version: entry_data.version,
        comment: entry_data.comment,
        categories: entry_data.categories,
        desktop_filename,
        icon_path,
        appimage_type,
        file_size_bytes,
        is_executable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_desktop_entry_and_slug() {
        let desktop = r#"
[Desktop Entry]
Name=Obsidian Note Taking
Comment=A powerful knowledge base
Icon=obsidian
Categories=Office;Utility;
X-AppImage-Version=1.7.4
Exec=obsidian %U
"#;
        let parsed = parse_desktop_entry(desktop);
        assert_eq!(parsed.name.as_deref(), Some("Obsidian Note Taking"));
        assert_eq!(parsed.comment.as_deref(), Some("A powerful knowledge base"));
        assert_eq!(parsed.version.as_deref(), Some("1.7.4"));
        assert_eq!(parsed.categories, vec!["Office", "Utility"]);
        assert_eq!(sanitize_slug("Obsidian Note Taking"), "obsidian-note-taking");
    }

    #[test]
    fn test_detect_elf_squashfs_payload() {
        let tmp = std::env::temp_dir().join(format!("pastor-appimg-test-{}.AppImage", std::process::id()));
        let mut f = File::create(&tmp).unwrap();
        // Minimal 64-byte ELF header with e_shoff=64, e_shentsize=64, e_shnum=1 -> end=128
        let mut elf = [0u8; 128];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2; // ELF64
        elf[5] = 1; // Little-endian
        elf[40..48].copy_from_slice(&64u64.to_le_bytes());
        elf[58..60].copy_from_slice(&64u16.to_le_bytes());
        elf[60..62].copy_from_slice(&1u16.to_le_bytes());
        f.write_all(&elf).unwrap();
        f.write_all(b"hsqs\x00\x00\x00\x00").unwrap();
        drop(f);

        let (kind, off) = detect_appimage_payload(&tmp);
        assert_eq!(kind, AppImageType::Type2Squashfs);
        assert_eq!(off, Some(128));
        let _ = fs::remove_file(&tmp);
    }
}
