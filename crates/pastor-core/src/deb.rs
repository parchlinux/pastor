use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::error::PastorError;

/// Metadata extracted from a Debian `.deb` archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebMetadata {
    pub package: String,
    pub arch_pkgname: String,
    pub version: String,
    pub arch_pkgver: String,
    pub architecture: String,
    pub arch_architecture: String,
    pub maintainer: Option<String>,
    pub description: String,
    pub long_description: Option<String>,
    pub homepage: Option<String>,
    pub section: Option<String>,
    pub installed_size_bytes: u64,
    pub depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub has_maintainer_scripts: bool,
    pub file_size_bytes: u64,
}

/// An `ar` archive member header summary.
#[derive(Debug, Clone)]
struct ArEntry {
    name: String,
    offset: u64,
    size: u64,
}

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(prefix: &str) -> Result<Self, PastorError> {
        let dir = std::env::temp_dir().join(format!(
            "pastor-{}-{}-{}",
            prefix,
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::create_dir_all(&dir).map_err(|e| PastorError::InvalidOperation(format!("Failed to create temp directory: {e}")))?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Reads all member entries from a standard Unix `ar` archive.
fn read_ar_entries(file: &mut File) -> Result<Vec<ArEntry>, PastorError> {
    let mut magic = [0u8; 8];
    file.seek(SeekFrom::Start(0))
        .map_err(|e| PastorError::InvalidOperation(format!("Failed to seek .deb file: {e}")))?;
    file.read_exact(&mut magic)
        .map_err(|e| PastorError::InvalidOperation(format!("Invalid .deb archive (too small): {e}")))?;

    if &magic != b"!<arch>\n" {
        return Err(PastorError::InvalidOperation(
            "Not a valid Debian .deb archive (missing ar magic header)".into(),
        ));
    }

    let file_len = file
        .metadata()
        .map_err(|e| PastorError::InvalidOperation(format!("Cannot stat .deb file: {e}")))?
        .len();

    let mut entries = Vec::new();
    let mut pos: u64 = 8;

    while pos + 60 <= file_len {
        file.seek(SeekFrom::Start(pos))
            .map_err(|e| PastorError::InvalidOperation(format!("Seek error in .deb archive: {e}")))?;
        let mut header = [0u8; 60];
        if file.read_exact(&mut header).is_err() {
            break;
        }
        if &header[58..60] != b"`\n" {
            return Err(PastorError::InvalidOperation(
                "Corrupt ar member header inside .deb archive".into(),
            ));
        }

        let raw_name = String::from_utf8_lossy(&header[0..16]).trim().to_string();
        let size_str = String::from_utf8_lossy(&header[48..58]).trim().to_string();
        let member_size: u64 = size_str.parse().map_err(|_| {
            PastorError::InvalidOperation(format!("Invalid ar member size for '{raw_name}': '{size_str}'"))
        })?;

        let mut data_offset = pos + 60;
        let mut actual_size = member_size;
        let mut clean_name = raw_name.trim_end_matches('/').to_string();

        // Handle BSD-style extended filename `#1/<len>`
        if let Some(len_str) = raw_name.strip_prefix("#1/") {
            if let Ok(name_len) = len_str.trim().parse::<u64>() {
                if name_len <= member_size {
                    let mut name_buf = vec![0u8; name_len as usize];
                    file.read_exact(&mut name_buf).map_err(|e| {
                        PastorError::InvalidOperation(format!("Failed reading extended ar member name: {e}"))
                    })?;
                    clean_name = String::from_utf8_lossy(&name_buf)
                        .trim_end_matches('\0')
                        .trim()
                        .to_string();
                    data_offset += name_len;
                    actual_size -= name_len;
                }
            }
        }

        entries.push(ArEntry {
            name: clean_name,
            offset: data_offset,
            size: actual_size,
        });

        // ar members are 2-byte aligned
        let padded = member_size + (member_size % 2);
        pos = pos + 60 + padded;
    }

    Ok(entries)
}

fn copy_ar_entry_to_file(src: &mut File, entry: &ArEntry, dest_path: &Path) -> Result<(), PastorError> {
    src.seek(SeekFrom::Start(entry.offset))
        .map_err(|e| PastorError::InvalidOperation(format!("Failed to seek ar member '{}': {e}", entry.name)))?;
    let mut out = File::create(dest_path)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed to create temp file: {e}")))?;

    let mut remaining = entry.size;
    let mut buf = [0u8; 65536];
    while remaining > 0 {
        let to_read = remaining.min(buf.len() as u64) as usize;
        let n = src
            .read(&mut buf[..to_read])
            .map_err(|e| PastorError::InvalidOperation(format!("Failed reading ar member '{}': {e}", entry.name)))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(|e| PastorError::InvalidOperation(format!("Failed writing temp file: {e}")))?;
        remaining -= n as u64;
    }
    Ok(())
}

/// Sanitizes a Debian package name for Arch Linux `pkgname` rules (`[a-z0-9@._+-]`).
pub fn sanitize_arch_pkgname(name: &str) -> String {
    let sanitized: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(|c| c == '-' || c == '.');
    if trimmed.is_empty() {
        "deb-package".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Sanitizes a Debian version into an Arch-compatible `pkgver-pkgrel` string.
/// Arch requires `pkgver` to have no hyphens, spaces, or colons (except numeric `epoch:`).
pub fn sanitize_arch_pkgver(deb_version: &str) -> String {
    let raw = deb_version.trim();
    if raw.is_empty() {
        return "0.1.0-1".to_string();
    }

    let (epoch_prefix, rest) = if let Some((ep, r)) = raw.split_once(':') {
        if !ep.is_empty() && ep.chars().all(|c| c.is_ascii_digit()) {
            (format!("{ep}:"), r)
        } else {
            (String::new(), raw)
        }
    } else {
        (String::new(), raw)
    };

    let clean_ver: String = rest
        .chars()
        .map(|c| match c {
            '-' | '~' => '_',
            c if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '+' => c,
            _ => '_',
        })
        .collect();

    let clean_ver = clean_ver.trim_matches('_');
    let clean_ver = if clean_ver.is_empty() { "0.1.0" } else { clean_ver };
    format!("{epoch_prefix}{clean_ver}-1")
}

/// Maps a Debian architecture string to its Arch Linux equivalent.
pub fn map_debian_arch(deb_arch: &str) -> String {
    match deb_arch.trim().to_lowercase().as_str() {
        "amd64" | "x86_64" => "x86_64".to_string(),
        "arm64" | "aarch64" => "aarch64".to_string(),
        "armhf" | "armv7l" => "armv7h".to_string(),
        "i386" | "i686" => "i686".to_string(),
        "all" | "any" => "any".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => "any".to_string(),
    }
}

/// Returns the current host Arch Linux architecture (`x86_64`, `aarch64`, etc.).
pub fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "arm" => "armv7h",
        "x86" => "i686",
        other => other,
    }
}

/// Parses a Debian RFC-822 control file into a field map (keys lowercased).
fn parse_control_fields(content: &str) -> HashMap<String, String> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut current_key: Option<String> = None;

    for line in content.lines() {
        if line.trim().is_empty() && current_key.is_none() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(ref key) = current_key {
                let cont = line.trim();
                let entry = fields.entry(key.clone()).or_default();
                entry.push('\n');
                if cont != "." {
                    entry.push_str(cont);
                }
            }
        } else if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            fields.insert(key.clone(), val);
            current_key = Some(key);
        }
    }

    fields
}

fn split_dep_list(raw: Option<&String>) -> Vec<String> {
    let Some(s) = raw else {
        return Vec::new();
    };
    s.split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// Inspects a `.deb` package file and extracts its metadata without modifying the system.
pub fn inspect_deb(deb_path: &Path) -> Result<DebMetadata, PastorError> {
    if !deb_path.is_file() {
        return Err(PastorError::PackageNotFound(format!(
            "Debian package file not found: {}",
            deb_path.display()
        )));
    }

    let file_size_bytes = deb_path
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    let mut file = File::open(deb_path)
        .map_err(|e| PastorError::InvalidOperation(format!("Cannot open .deb file: {e}")))?;

    let entries = read_ar_entries(&mut file)?;

    let control_entry = entries
        .iter()
        .find(|e| e.name.starts_with("control.tar"))
        .ok_or_else(|| PastorError::InvalidOperation("Missing control.tar inside .deb archive".into()))?;

    let _data_entry = entries
        .iter()
        .find(|e| e.name.starts_with("data.tar"))
        .ok_or_else(|| PastorError::InvalidOperation("Missing data.tar inside .deb archive".into()))?;

    let temp = TempDirGuard::new("deb-inspect")?;
    let control_tar_path = temp.path().join(&control_entry.name);
    copy_ar_entry_to_file(&mut file, control_entry, &control_tar_path)?;

    let control_dir = temp.path().join("control");
    fs::create_dir_all(&control_dir)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed to create control extract dir: {e}")))?;

    let extract_status = Command::new("bsdtar")
        .arg("-xf")
        .arg(&control_tar_path)
        .arg("-C")
        .arg(&control_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed to run bsdtar on control archive: {e}")))?;

    if !extract_status.success() {
        return Err(PastorError::InvalidOperation(
            "Failed to extract control metadata from .deb archive".into(),
        ));
    }

    let control_file_path = control_dir.join("control");
    let control_text = fs::read_to_string(&control_file_path)
        .map_err(|e| PastorError::InvalidOperation(format!("Missing or unreadable 'control' file in .deb: {e}")))?;

    let fields = parse_control_fields(&control_text);
    let package = fields
        .get("package")
        .cloned()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| PastorError::InvalidOperation("Debian control file is missing 'Package' field".into()))?;

    let version = fields
        .get("version")
        .cloned()
        .unwrap_or_else(|| "0.1.0".to_string());
    let architecture = fields
        .get("architecture")
        .cloned()
        .unwrap_or_else(|| "all".to_string());

    let raw_desc = fields
        .get("description")
        .cloned()
        .unwrap_or_else(|| format!("Converted Debian package ({package})"));
    let mut desc_lines = raw_desc.lines();
    let short_desc = desc_lines.next().unwrap_or("").trim().to_string();
    let rest_desc: Vec<&str> = desc_lines.collect();
    let long_description = if rest_desc.is_empty() {
        None
    } else {
        Some(rest_desc.join("\n").trim().to_string()).filter(|s| !s.is_empty())
    };

    let installed_kb: u64 = fields
        .get("installed-size")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let has_maintainer_scripts = ["postinst", "preinst", "prerm", "postrm"]
        .iter()
        .any(|s| control_dir.join(s).exists());

    Ok(DebMetadata {
        arch_pkgname: sanitize_arch_pkgname(&package),
        arch_pkgver: sanitize_arch_pkgver(&version),
        arch_architecture: map_debian_arch(&architecture),
        package,
        version,
        architecture,
        maintainer: fields.get("maintainer").cloned().filter(|s| !s.is_empty()),
        description: if short_desc.is_empty() {
            "Converted Debian package".to_string()
        } else {
            short_desc
        },
        long_description,
        homepage: fields.get("homepage").cloned().filter(|s| !s.is_empty()),
        section: fields.get("section").cloned().filter(|s| !s.is_empty()),
        installed_size_bytes: installed_kb * 1024,
        depends: split_dep_list(fields.get("depends")),
        provides: split_dep_list(fields.get("provides")),
        conflicts: split_dep_list(fields.get("conflicts")),
        has_maintainer_scripts,
        file_size_bytes,
    })
}

/// Lookup table mapping common Debian package names to Arch Linux package names.
fn static_deb_to_arch_dep(deb_name: &str) -> Option<&'static str> {
    match deb_name {
        "libc6" | "libc6-amd64" => Some("glibc"),
        "libstdc++6" | "libgcc-s1" | "libgcc1" => Some("gcc-libs"),
        "zlib1g" => Some("zlib"),
        "libssl3" | "libssl3t64" | "libssl1.1" | "libssl1.0.0" => Some("openssl"),
        "libgtk-3-0" | "libgtk-3-0t64" | "libgtk-3-common" => Some("gtk3"),
        "libgtk-4-1" | "libgtk-4-common" => Some("gtk4"),
        "libgtk2.0-0" => Some("gtk2"),
        "libadwaita-1-0" => Some("libadwaita"),
        "libglib2.0-0" | "libglib2.0-0t64" => Some("glib2"),
        "libgdk-pixbuf-2.0-0" | "libgdk-pixbuf2.0-0" => Some("gdk-pixbuf2"),
        "libpango-1.0-0" | "libpangocairo-1.0-0" | "libpangoft2-1.0-0" => Some("pango"),
        "libcairo2" | "libcairo-gobject2" => Some("cairo"),
        "libharfbuzz0b" => Some("harfbuzz"),
        "libfontconfig1" => Some("fontconfig"),
        "libfreetype6" => Some("freetype2"),
        "libexpat1" => Some("expat"),
        "libnss3" => Some("nss"),
        "libnspr4" => Some("nspr"),
        "libasound2" | "libasound2t64" => Some("alsa-lib"),
        "libpulse0" => Some("libpulse"),
        "libpipewire-0.3-0" => Some("pipewire"),
        "libx11-6" | "libx11-xcb1" => Some("libx11"),
        "libxext6" => Some("libxext"),
        "libxfixes3" => Some("libxfixes"),
        "libxi6" => Some("libxi"),
        "libxrandr2" => Some("libxrandr"),
        "libxrender1" => Some("libxrender"),
        "libxtst6" => Some("libxtst"),
        "libxss1" => Some("libxss"),
        "libxcursor1" => Some("libxcursor"),
        "libxcomposite1" => Some("libxcomposite"),
        "libxdamage1" => Some("libxdamage"),
        "libxinerama1" => Some("libxinerama"),
        "libxshmfence1" => Some("libxshmfence"),
        "libxcb1" | "libxcb-dri3-0" | "libxcb-glx0" => Some("libxcb"),
        "libxkbcommon0" => Some("libxkbcommon"),
        "libxkbcommon-x11-0" => Some("libxkbcommon-x11"),
        "libwayland-client0" | "libwayland-cursor0" | "libwayland-egl1" => Some("wayland"),
        "libgbm1" | "libegl1" | "libegl-mesa0" | "libgl1-mesa-glx" => Some("mesa"),
        "libgl1" => Some("libglvnd"),
        "libdrm2" => Some("libdrm"),
        "libvulkan1" => Some("vulkan-icd-loader"),
        "libcups2" | "libcups2t64" => Some("libcups"),
        "libdbus-1-3" => Some("dbus"),
        "libatspi2.0-0" | "libatspi2.0-0t64" | "libatk1.0-0" | "libatk-bridge2.0-0" => Some("at-spi2-core"),
        "libnotify4" => Some("libnotify"),
        "libsecret-1-0" => Some("libsecret"),
        "libcurl4" | "libcurl4t64" | "libcurl3-gnutls" => Some("curl"),
        "libsqlite3-0" => Some("sqlite"),
        "libxml2" => Some("libxml2"),
        "libxslt1.1" => Some("libxslt"),
        "libuuid1" => Some("util-linux-libs"),
        "libpcre3" => Some("pcre"),
        "libpcre2-8-0" => Some("pcre2"),
        "libappindicator3-1" | "libayatana-appindicator3-1" => Some("libappindicator-gtk3"),
        "libwebkit2gtk-4.0-37" => Some("webkit2gtk"),
        "libwebkit2gtk-4.1-0" => Some("webkit2gtk-4.1"),
        "libfuse2" | "libfuse2t64" => Some("fuse2"),
        "libfuse3-3" => Some("fuse3"),
        "libudev1" => Some("systemd-libs"),
        "libbz2-1.0" => Some("bzip2"),
        "liblzma5" => Some("xz"),
        "libzstd1" => Some("zstd"),
        "libpng16-16" | "libpng16-16t64" => Some("libpng"),
        "libjpeg62-turbo" | "libjpeg-turbo8" | "libjpeg8" => Some("libjpeg-turbo"),
        "libwebp7" => Some("libwebp"),
        "libtiff6" | "libtiff5" => Some("libtiff"),
        "libopenjp2-7" => Some("openjpeg2"),
        "liblcms2-2" => Some("lcms2"),
        "libicu70" | "libicu72" | "libicu74" | "libicu76" => Some("icu"),
        "libffi8" | "libffi7" => Some("libffi"),
        "libgmp10" => Some("gmp"),
        "libgnutls30" | "libgnutls30t64" => Some("gnutls"),
        "libkrb5-3" => Some("krb5"),
        "libldap-2.5-0" | "libldap2" => Some("libldap"),
        "libsasl2-2" => Some("libsasl"),
        "libssh2-1" => Some("libssh2"),
        "libnghttp2-14" => Some("libnghttp2"),
        "libpsl5" | "libpsl5t64" => Some("libpsl"),
        "libgstreamer1.0-0" => Some("gstreamer"),
        "libgstreamer-plugins-base1.0-0" | "gstreamer1.0-plugins-base" => Some("gst-plugins-base"),
        "gstreamer1.0-plugins-good" => Some("gst-plugins-good"),
        "gstreamer1.0-plugins-bad" => Some("gst-plugins-bad"),
        "gstreamer1.0-plugins-ugly" => Some("gst-plugins-ugly"),
        "gstreamer1.0-libav" => Some("gst-libav"),
        "libva2" | "libva-drm2" | "libva-x11-2" => Some("libva"),
        "libvdpau1" => Some("libvdpau"),
        "libsdl2-2.0-0" => Some("sdl2"),
        "python3" | "python3-minimal" => Some("python"),
        "python3-gi" => Some("python-gobject"),
        "python3-requests" => Some("python-requests"),
        "python3-yaml" => Some("python-yaml"),
        "xdg-utils" => Some("xdg-utils"),
        "desktop-file-utils" => Some("desktop-file-utils"),
        "hicolor-icon-theme" => Some("hicolor-icon-theme"),
        "shared-mime-info" => Some("shared-mime-info"),
        "ca-certificates" => Some("ca-certificates"),
        "bash" => Some("bash"),
        "sh" | "dash" => Some("sh"),
        "coreutils" => Some("coreutils"),
        "grep" => Some("grep"),
        "sed" => Some("sed"),
        "gawk" => Some("gawk"),
        "tar" => Some("tar"),
        "unzip" => Some("unzip"),
        "curl" => Some("curl"),
        "wget" => Some("wget"),
        "git" => Some("git"),
        "ffmpeg" => Some("ffmpeg"),
        "libxt6" => Some("libxt"),
        "libice6" => Some("libice"),
        "libsm6" => Some("libsm"),
        "libpci3" => Some("pciutils"),
        "libcanberra0" | "libcanberra-gtk3-0" => Some("libcanberra"),
        "libvpx7" | "libvpx9" => Some("libvpx"),
        "libopus0" => Some("opus"),
        "libflac12" | "libflac8" => Some("flac"),
        "libvorbis0a" | "libvorbisenc2" | "libvorbisfile3" => Some("libvorbis"),
        "libogg0" => Some("libogg"),
        _ => None,
    }
}

/// Strips version constraints `(>= 1.2)` and architecture qualifiers `:any` from a single
/// Debian dependency atom, then returns ordered candidate Arch package names.
pub fn map_debian_dep_candidates(dep_atom: &str) -> Vec<String> {
    let no_ver = dep_atom.split('(').next().unwrap_or(dep_atom).trim();
    let base = no_ver.split(':').next().unwrap_or(no_ver).trim().to_lowercase();
    if base.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    if let Some(mapped) = static_deb_to_arch_dep(&base) {
        candidates.push(mapped.to_string());
    }

    candidates.push(base.clone());

    // Heuristic: strip Debian ABI/time64 suffixes like `t64`, `-0`, `-1`, `-2`
    let stripped_t64 = base.strip_suffix("t64").unwrap_or(&base);
    if stripped_t64 != base {
        if let Some(mapped) = static_deb_to_arch_dep(stripped_t64) {
            candidates.push(mapped.to_string());
        }
        candidates.push(stripped_t64.to_string());
    }

    // Heuristic: `libfoo-1-0` or `libfoo2` -> `foo` / `libfoo`
    if let Some(without_abi) = stripped_t64.rsplit_once('-').and_then(|(prefix, suffix)| {
        if suffix.chars().all(|c| c.is_ascii_digit() || c == '.') {
            Some(prefix)
        } else {
            None
        }
    }) {
        candidates.push(without_abi.to_string());
        if let Some(without_lib) = without_abi.strip_prefix("lib") {
            candidates.push(without_lib.to_string());
        }
    }

    // Deduplicate preserving order
    let mut seen = BTreeSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    candidates
}

/// Fallback heuristic mapping when no live ALPM database handle is available.
pub fn map_debian_depends_offline(deb_depends: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for spec in deb_depends {
        // Take the first alternative in `a | b` that maps cleanly
        for alt in spec.split('|') {
            let no_ver = alt.split('(').next().unwrap_or(alt).trim();
            let base = no_ver.split(':').next().unwrap_or(no_ver).trim().to_lowercase();
            if let Some(mapped) = static_deb_to_arch_dep(&base) {
                out.insert(mapped.to_string());
                break;
            }
        }
    }
    out.into_iter().collect()
}

/// Generates smart search queries from a `.deb` package's metadata to check for
/// existing Flatpak, AUR, or Official Repository alternatives.
pub fn search_candidates_for_deb(meta: &DebMetadata) -> Vec<String> {
    let mut queries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push_unique = |s: String| {
        let s = s.trim().to_lowercase();
        if s.len() >= 2 && seen.insert(s.clone()) {
            queries.push(s);
        }
    };

    let raw = meta.package.trim().to_lowercase();
    push_unique(raw.clone());

    // Common Debian package aliases -> upstream Arch/Flatpak names
    let alias = match raw.as_str() {
        "code" | "code-insiders" => Some("visual-studio-code"),
        "google-chrome-stable" | "google-chrome-beta" | "google-chrome-unstable" => Some("google-chrome"),
        "microsoft-edge-stable" | "microsoft-edge-beta" => Some("microsoft-edge"),
        "slack-desktop" => Some("slack"),
        "spotify-client" => Some("spotify"),
        "telegram-desktop" => Some("telegram-desktop"),
        "zoom" => Some("zoom"),
        "discord" | "discord-canary" | "discord-ptb" => Some("discord"),
        "1password" => Some("1password"),
        "brave-browser" => Some("brave"),
        "vivaldi-stable" => Some("vivaldi"),
        "opera-stable" => Some("opera"),
        "sublime-text" => Some("sublime-text"),
        "steam-launcher" => Some("steam"),
        "obs-studio" => Some("obs-studio"),
        "obsidian" => Some("obsidian"),
        "anydesk" => Some("anydesk"),
        "teamviewer" => Some("teamviewer"),
        "insomnia" => Some("insomnia"),
        "postman" => Some("postman"),
        "figma-linux" => Some("figma-linux"),
        _ => None,
    };
    if let Some(a) = alias {
        push_unique(a.to_string());
    }

    // Strip common packaging suffixes
    for suffix in &[
        "-stable",
        "-desktop",
        "-client",
        "-linux",
        "-browser",
        "-bin",
        "-amd64",
        "-arm64",
        "-x86_64",
        "-ce",
        "-oss",
    ] {
        if let Some(stripped) = raw.strip_suffix(suffix) {
            push_unique(stripped.to_string());
        }
    }

    queries
}

/// Recursively merges `src` into `dst` (used for Arch merged-`/usr` normalization)
/// and removes `src`.
fn merge_dir_into(src: &Path, dst: &Path) -> Result<(), PastorError> {
    if !src.exists() {
        return Ok(());
    }
    // If src itself is a symlink (e.g. /lib -> usr/lib), just remove the symlink
    let meta = fs::symlink_metadata(src)
        .map_err(|e| PastorError::InvalidOperation(format!("Stat error during path normalization: {e}")))?;
    if meta.file_type().is_symlink() {
        fs::remove_file(src).ok();
        return Ok(());
    }
    if !meta.is_dir() {
        return Ok(());
    }

    fs::create_dir_all(dst)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed creating target dir {}: {e}", dst.display())))?;

    for entry in fs::read_dir(src)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed reading dir {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| PastorError::InvalidOperation(format!("Dir entry error: {e}")))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let entry_meta = fs::symlink_metadata(&from)
            .map_err(|e| PastorError::InvalidOperation(format!("Symlink metadata error: {e}")))?;

        if entry_meta.is_dir() && !entry_meta.file_type().is_symlink() {
            merge_dir_into(&from, &to)?;
        } else {
            if to.exists() || fs::symlink_metadata(&to).is_ok() {
                let _ = fs::remove_file(&to);
            }
            if fs::rename(&from, &to).is_err() {
                // Cross-device fallback
                if entry_meta.file_type().is_symlink() {
                    let target = fs::read_link(&from)
                        .map_err(|e| PastorError::InvalidOperation(format!("Read symlink failed: {e}")))?;
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(target, &to)
                        .map_err(|e| PastorError::InvalidOperation(format!("Write symlink failed: {e}")))?;
                    let _ = fs::remove_file(&from);
                } else {
                    fs::copy(&from, &to)
                        .map_err(|e| PastorError::InvalidOperation(format!("Copy failed: {e}")))?;
                    let _ = fs::remove_file(&from);
                }
            }
        }
    }

    let _ = fs::remove_dir_all(src);
    Ok(())
}

/// Normalizes Debian directory layout into Arch Linux merged-`/usr` conventions.
fn normalize_arch_filesystem_layout(root: &Path) -> Result<(), PastorError> {
    let moves = [
        (root.join("bin"), root.join("usr/bin")),
        (root.join("sbin"), root.join("usr/bin")),
        (root.join("usr/sbin"), root.join("usr/bin")),
        (root.join("lib"), root.join("usr/lib")),
        (root.join("lib64"), root.join("usr/lib")),
        (root.join("usr/lib64"), root.join("usr/lib")),
        (root.join("usr/lib/x86_64-linux-gnu"), root.join("usr/lib")),
        (root.join("usr/lib/aarch64-linux-gnu"), root.join("usr/lib")),
    ];

    for (src, dst) in moves {
        if fs::symlink_metadata(&src).is_ok() {
            merge_dir_into(&src, &dst)?;
        }
    }

    Ok(())
}

fn dir_size_bytes(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let Ok(meta) = fs::symlink_metadata(&p) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            total = total.saturating_add(dir_size_bytes(&p));
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Converts a `.deb` package into an Arch Linux `.pkg.tar.zst` package in `output_dir`.
/// Returns the full path to the generated `.pkg.tar.zst` file.
pub fn convert_deb_to_pkg(
    deb_path: &Path,
    output_dir: &Path,
    resolved_depends: &[String],
) -> Result<PathBuf, PastorError> {
    let meta = inspect_deb(deb_path)?;

    let mut file = File::open(deb_path)
        .map_err(|e| PastorError::InvalidOperation(format!("Cannot open .deb file: {e}")))?;
    let entries = read_ar_entries(&mut file)?;
    let data_entry = entries
        .iter()
        .find(|e| e.name.starts_with("data.tar"))
        .ok_or_else(|| PastorError::InvalidOperation("Missing data.tar inside .deb archive".into()))?;

    let temp = TempDirGuard::new("deb-convert")?;
    let data_tar_path = temp.path().join(&data_entry.name);
    copy_ar_entry_to_file(&mut file, data_entry, &data_tar_path)?;

    let staging = temp.path().join("pkg");
    fs::create_dir_all(&staging)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed creating staging dir: {e}")))?;

    // Extract payload
    let extract_status = Command::new("bsdtar")
        .arg("-xf")
        .arg(&data_tar_path)
        .arg("-C")
        .arg(&staging)
        .arg("--no-same-owner")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed to execute bsdtar: {e}")))?;

    if !extract_status.success() {
        return Err(PastorError::InvalidOperation(
            "Failed to extract payload (data.tar) from .deb archive".into(),
        ));
    }

    normalize_arch_filesystem_layout(&staging)?;

    let computed_size = dir_size_bytes(&staging).max(meta.installed_size_bytes);
    let builddate = chrono::Utc::now().timestamp();

    // Write .PKGINFO
    let mut pkginfo = String::new();
    pkginfo.push_str("# Generated by Parch Store (pastor) .deb converter\n");
    pkginfo.push_str(&format!("pkgname = {}\n", meta.arch_pkgname));
    pkginfo.push_str(&format!("pkgbase = {}\n", meta.arch_pkgname));
    pkginfo.push_str(&format!("pkgver = {}\n", meta.arch_pkgver));
    pkginfo.push_str(&format!(
        "pkgdesc = {}\n",
        meta.description.replace(['\n', '\r'], " ")
    ));
    if let Some(ref url) = meta.homepage {
        pkginfo.push_str(&format!("url = {}\n", url.trim()));
    }
    pkginfo.push_str(&format!("builddate = {builddate}\n"));
    pkginfo.push_str("packager = Parch Store <pastor@parchlinux.com>\n");
    pkginfo.push_str(&format!("size = {computed_size}\n"));
    pkginfo.push_str(&format!("arch = {}\n", meta.arch_architecture));
    pkginfo.push_str("license = custom\n");
    for dep in resolved_depends {
        pkginfo.push_str(&format!("depend = {dep}\n"));
    }

    fs::write(staging.join(".PKGINFO"), pkginfo)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed writing .PKGINFO: {e}")))?;

    // Collect relative entry list inside staging for .MTREE and final tar
    let mut top_entries: Vec<String> = fs::read_dir(&staging)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed reading staging root: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    top_entries.sort();

    // Generate .MTREE
    let mtree_status = Command::new("bsdtar")
        .current_dir(&staging)
        .arg("-czf")
        .arg(".MTREE")
        .arg("--format=mtree")
        .arg("--options=!all,use-set,type,uid,gid,mode,time,size,md5,sha256,link")
        .arg("--uname=root")
        .arg("--gname=root")
        .args(&top_entries)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed generating .MTREE: {e}")))?;

    if !mtree_status.success() {
        return Err(PastorError::InvalidOperation(
            "bsdtar failed while generating .MTREE metadata".into(),
        ));
    }

    // Rebuild sorted entry list now including .MTREE
    let mut final_entries: Vec<String> = fs::read_dir(&staging)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed reading staging root: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    final_entries.sort();

    fs::create_dir_all(output_dir)
        .map_err(|e| PastorError::InvalidOperation(format!("Failed creating output dir: {e}")))?;

    let safe_ver_for_filename = meta.arch_pkgver.replace(':', "_");
    let pkg_filename = format!(
        "{}-{}-{}.pkg.tar.zst",
        meta.arch_pkgname, safe_ver_for_filename, meta.arch_architecture
    );
    let out_path = output_dir.join(pkg_filename);

    // Pipe bsdtar -> zstd to produce .pkg.tar.zst
    let mut tar_child = Command::new("bsdtar")
        .current_dir(&staging)
        .arg("-cf")
        .arg("-")
        .arg("--uname=root")
        .arg("--gname=root")
        .args(&final_entries)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed spawning bsdtar: {e}")))?;

    let tar_stdout = tar_child
        .stdout
        .take()
        .ok_or_else(|| PastorError::InvalidOperation("Failed capturing bsdtar stdout".into()))?;

    let zstd_status = Command::new("zstd")
        .arg("-T0")
        .arg("-3")
        .arg("-q")
        .arg("-f")
        .arg("-o")
        .arg(&out_path)
        .stdin(tar_stdout)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed running zstd: {e}")))?;

    let tar_status = tar_child
        .wait()
        .map_err(|e| PastorError::InvalidOperation(format!("Failed waiting for bsdtar: {e}")))?;

    if !tar_status.success() || !zstd_status.success() || !out_path.is_file() {
        return Err(PastorError::InvalidOperation(
            "Failed assembling .pkg.tar.zst archive".into(),
        ));
    }

    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_ar_member(out: &mut File, name: &str, data: &[u8]) {
        let mut header = [b' '; 60];
        let name_with_slash = format!("{name}/");
        let name_bytes = name_with_slash.as_bytes();
        let copy_len = name_bytes.len().min(16);
        header[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        let size_str = format!("{}", data.len());
        header[48..48 + size_str.len()].copy_from_slice(size_str.as_bytes());
        header[58..60].copy_from_slice(b"`\n");
        out.write_all(&header).unwrap();
        out.write_all(data).unwrap();
        if data.len() % 2 == 1 {
            out.write_all(b"\n").unwrap();
        }
    }

    #[test]
    fn test_sanitize_pkgname_and_pkgver() {
        assert_eq!(sanitize_arch_pkgname("Google-Chrome-Stable"), "google-chrome-stable");
        assert_eq!(sanitize_arch_pkgname("foo_bar+1.0"), "foo_bar+1.0");
        assert_eq!(sanitize_arch_pkgver("1:1.2.3-4ubuntu1"), "1:1.2.3_4ubuntu1-1");
        assert_eq!(sanitize_arch_pkgver("0.0.75"), "0.0.75-1");
        assert_eq!(sanitize_arch_pkgver("2.4.0~rc1-1"), "2.4.0_rc1_1-1");
        assert_eq!(map_debian_arch("amd64"), "x86_64");
        assert_eq!(map_debian_arch("arm64"), "aarch64");
        assert_eq!(map_debian_arch("all"), "any");
    }

    #[test]
    fn test_dep_mapping_and_search_candidates() {
        let candidates = map_debian_dep_candidates("libgtk-3-0 (>= 3.24)");
        assert!(candidates.contains(&"gtk3".to_string()));

        let t64 = map_debian_dep_candidates("libasound2t64:amd64 (>= 1.0)");
        assert!(t64.contains(&"alsa-lib".to_string()));

        let meta = DebMetadata {
            package: "google-chrome-stable".into(),
            arch_pkgname: "google-chrome-stable".into(),
            version: "130.0.1-1".into(),
            arch_pkgver: "130.0.1_1-1".into(),
            architecture: "amd64".into(),
            arch_architecture: "x86_64".into(),
            maintainer: None,
            description: "Browser".into(),
            long_description: None,
            homepage: None,
            section: None,
            installed_size_bytes: 0,
            depends: vec![],
            provides: vec![],
            conflicts: vec![],
            has_maintainer_scripts: false,
            file_size_bytes: 0,
        };
        let queries = search_candidates_for_deb(&meta);
        assert!(queries.contains(&"google-chrome-stable".to_string()));
        assert!(queries.contains(&"google-chrome".to_string()));
    }

    #[test]
    fn test_synthetic_deb_inspect_and_convert_roundtrip() {
        let temp = TempDirGuard::new("deb-test").unwrap();

        // Build synthetic control.tar.gz
        let ctrl_dir = temp.path().join("ctrl_src");
        fs::create_dir_all(&ctrl_dir).unwrap();
        fs::write(
            ctrl_dir.join("control"),
            "Package: hello-parch\nVersion: 1.2.3-4deb1\nArchitecture: amd64\nMaintainer: Test <test@example.com>\nDepends: libc6 (>= 2.34), libgtk-4-1\nHomepage: https://parchlinux.com\nDescription: Sample greeting app\n Long multi-line description here.\n",
        )
        .unwrap();
        let ctrl_tar = temp.path().join("control.tar.gz");
        let st = Command::new("bsdtar")
            .current_dir(&ctrl_dir)
            .args(["-czf", ctrl_tar.to_str().unwrap(), "control"])
            .status()
            .unwrap();
        assert!(st.success());

        // Build synthetic data.tar.gz with /bin and /usr/lib/x86_64-linux-gnu to test normalization
        let data_dir = temp.path().join("data_src");
        fs::create_dir_all(data_dir.join("bin")).unwrap();
        fs::create_dir_all(data_dir.join("usr/lib/x86_64-linux-gnu")).unwrap();
        fs::write(data_dir.join("bin/hello-parch"), b"#!/bin/sh\necho hello\n").unwrap();
        fs::write(data_dir.join("usr/lib/x86_64-linux-gnu/libhello.so"), b"ELF").unwrap();
        let data_tar = temp.path().join("data.tar.gz");
        let st2 = Command::new("bsdtar")
            .current_dir(&data_dir)
            .args(["-czf", data_tar.to_str().unwrap(), "bin", "usr"])
            .status()
            .unwrap();
        assert!(st2.success());

        // Assemble .deb ar archive
        let deb_path = temp.path().join("hello-parch_1.2.3_amd64.deb");
        let mut deb_file = File::create(&deb_path).unwrap();
        deb_file.write_all(b"!<arch>\n").unwrap();
        write_ar_member(&mut deb_file, "debian-binary", b"2.0\n");
        write_ar_member(&mut deb_file, "control.tar.gz", &fs::read(&ctrl_tar).unwrap());
        write_ar_member(&mut deb_file, "data.tar.gz", &fs::read(&data_tar).unwrap());
        drop(deb_file);

        // Inspect
        let meta = inspect_deb(&deb_path).unwrap();
        assert_eq!(meta.package, "hello-parch");
        assert_eq!(meta.arch_pkgname, "hello-parch");
        assert_eq!(meta.arch_pkgver, "1.2.3_4deb1-1");
        assert_eq!(meta.arch_architecture, "x86_64");
        assert_eq!(meta.description, "Sample greeting app");
        assert_eq!(meta.long_description.as_deref(), Some("Long multi-line description here."));

        // Convert
        let deps = map_debian_depends_offline(&meta.depends);
        assert!(deps.contains(&"glibc".to_string()));
        assert!(deps.contains(&"gtk4".to_string()));

        let out_dir = temp.path().join("out");
        let pkg_path = convert_deb_to_pkg(&deb_path, &out_dir, &deps).unwrap();
        assert!(pkg_path.is_file());
        assert!(pkg_path.to_string_lossy().ends_with(".pkg.tar.zst"));

        // Verify .PKGINFO, .MTREE, and merged-/usr paths in the generated .pkg.tar.zst
        let list_out = Command::new("bsdtar")
            .args(["-tf", pkg_path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(list_out.status.success());
        let listing = String::from_utf8_lossy(&list_out.stdout);
        assert!(listing.contains(".PKGINFO"));
        assert!(listing.contains(".MTREE"));
        assert!(listing.contains("usr/bin/hello-parch"));
        assert!(listing.contains("usr/lib/libhello.so"));
        assert!(!listing.contains("\nbin/"));
    }
}
