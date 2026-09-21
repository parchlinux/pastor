use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use pastor_core::{PackageCategory, PackageIcon};
use quick_xml::{events::Event, reader::Reader};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlpmAppMetadata {
    pub pkgname: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: Option<String>,
    pub icon: Option<PackageIcon>,
    pub icon_cached_file: Option<String>,
    pub icon_theme_name: Option<String>,
    pub categories: Vec<PackageCategory>,
    pub screenshots: Vec<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub developer: Option<String>,
    pub launchables: Vec<String>,
    pub provides_ids: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AlpmCatalog {
    /// Maps pkgname -> App metadata
    pub by_pkgname: HashMap<String, AlpmAppMetadata>,
    /// Maps desktop id (e.g. gimp.desktop) -> App metadata
    pub by_id: HashMap<String, AlpmAppMetadata>,
    pub icons_base: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct AlpmCacheFile {
    signature: u64,
    catalog: AlpmCatalog,
}

fn compute_swcatalog_signature() -> u64 {
    let mut sig: u64 = 0x53574b5744; // SWKWD v2 cache salt
    let xml_dir = Path::new("/usr/share/swcatalog/xml");
    if let Ok(entries) = std::fs::read_dir(xml_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                sig = sig.wrapping_add(meta.len());
                if let Ok(modified) = meta.modified() {
                    if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                        sig = sig.wrapping_add(dur.as_secs()).wrapping_add(dur.subsec_nanos() as u64);
                    }
                }
            }
        }
    }
    if let Ok(meta) = std::fs::metadata("/usr/share/applications") {
        if let Ok(modified) = meta.modified() {
            if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                sig = sig.wrapping_add(dur.as_secs()).wrapping_add(dur.subsec_nanos() as u64);
            }
        }
    }
    sig
}

fn get_catalog_cache_file() -> PathBuf {
    let cache_home = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".cache")
        });
    let dir = cache_home.join("pastor");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("alpm_catalog_v2.bin")
}

impl AlpmCatalog {
    pub fn load_from_cache() -> Option<Self> {
        let path = get_catalog_cache_file();
        let file = File::open(&path).ok()?;
        let reader = BufReader::new(file);
        let cached: AlpmCacheFile = bincode::deserialize_from(reader).ok()?;
        let current_sig = compute_swcatalog_signature();
        if cached.signature == current_sig && !cached.catalog.by_pkgname.is_empty() {
            tracing::info!(
                "AlpmCatalog loaded from binary cache: {} packages in <10ms",
                cached.catalog.by_pkgname.len()
            );
            Some(cached.catalog)
        } else {
            None
        }
    }

    pub fn save_to_cache(&self) {
        let path = get_catalog_cache_file();
        let sig = compute_swcatalog_signature();
        let data = AlpmCacheFile {
            signature: sig,
            catalog: self.clone(),
        };
        if let Ok(file) = File::create(&path) {
            let writer = std::io::BufWriter::new(file);
            let _ = bincode::serialize_into(writer, &data);
            tracing::info!("AlpmCatalog binary cache written to {:?}", path);
        }
    }

    pub fn load_system() -> Self {
        if let Some(cached) = Self::load_from_cache() {
            return cached;
        }

        let catalog = Self::parse_system_raw();
        catalog.save_to_cache();
        catalog
    }

    pub fn parse_system_raw() -> Self {
        let mut catalog = Self::default();

        let icons_dir = Path::new("/usr/share/swcatalog/icons");
        if icons_dir.is_dir() {
            catalog.icons_base = Some(icons_dir.to_path_buf());
        }

        let xml_dir = Path::new("/usr/share/swcatalog/xml");
        if xml_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(xml_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if name.ends_with(".xml.gz") {
                        if let Ok(file) = File::open(&path) {
                            let gz = GzDecoder::new(BufReader::new(file));
                            catalog.parse_xml(BufReader::new(gz));
                        }
                    } else if name.ends_with(".xml") {
                        if let Ok(file) = File::open(&path) {
                            catalog.parse_xml(BufReader::new(file));
                        }
                    }
                }
            }
        }

        // Also enrich with /usr/share/applications/*.desktop if any apps are missing
        catalog.enrich_from_desktop_files(Path::new("/usr/share/applications"));

        tracing::info!(
            "AlpmCatalog loaded: {} packages with AppStream/desktop metadata",
            catalog.by_pkgname.len()
        );

        catalog
    }

    pub fn get(&self, name: &str) -> Option<AlpmAppMetadata> {
        let mut meta = self.by_pkgname.get(name)
            .or_else(|| self.by_id.get(name))
            .or_else(|| {
                let clean = name.strip_suffix(".desktop").unwrap_or(name);
                self.by_pkgname.get(clean)
                    .or_else(|| self.by_id.get(clean))
            })
            .or_else(|| {
                let lower = name.to_lowercase();
                self.by_pkgname.get(&lower)
                    .or_else(|| self.by_id.get(&lower))
                    .or_else(|| {
                        let clean = lower.strip_suffix(".desktop").unwrap_or(&lower);
                        self.by_pkgname.get(clean)
                            .or_else(|| self.by_id.get(clean))
                    })
            })
            .cloned()?;

        if let Some(ref cached_file) = meta.icon_cached_file {
            if let Some(ref base) = self.icons_base {
                if let Some(local_path) = resolve_cached_icon(base, cached_file) {
                    meta.icon = Some(PackageIcon::LocalPath(local_path));
                }
            }
        }
        if meta.icon.is_none() {
            if let Some(ref theme_name) = meta.icon_theme_name {
                meta.icon = Some(PackageIcon::Themed(theme_name.clone()));
            }
        }

        Some(meta)
    }

    pub fn parse_xml<R: std::io::BufRead>(&mut self, buf_reader: R) {
        let mut reader = Reader::from_reader(buf_reader);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::with_capacity(4096);
        let mut in_component = false;
        let mut in_provides = false;
        let mut comp = AlpmAppMetadata::default();

        let mut current_tag = String::new();
        let mut current_type = String::new();
        let mut current_text = String::new();
        let mut is_default_lang = true;

        while let Ok(event) = reader.read_event_into(&mut buf) {
            match event {
                Event::Start(ref e) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    current_tag = name.clone();
                    current_type.clear();

                    if name == "component" {
                        in_component = true;
                        comp = AlpmAppMetadata::default();
                    } else if name == "provides" {
                        in_provides = true;
                    } else if in_component {
                        is_default_lang = true;
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            let val = String::from_utf8_lossy(&attr.value);
                            if key == "xml:lang" {
                                let lang = val.trim();
                                if !lang.is_empty() && lang != "C" && !lang.starts_with("en") {
                                    is_default_lang = false;
                                }
                            } else if key == "type" {
                                current_type = val.to_string();
                            }
                        }
                    }
                    current_text.clear();
                }
                Event::Text(ref e) if in_component => {
                    if let Ok(txt) = e.unescape() {
                        current_text.push_str(&txt);
                    }
                }
                Event::End(ref e) => {
                    let name_bytes = e.name();
                    let name = String::from_utf8_lossy(name_bytes.as_ref()).to_string();

                    if name == "component" {
                        self.index_component(comp.clone());
                        in_component = false;
                    } else if name == "provides" {
                        in_provides = false;
                    } else if in_component {
                        let text = current_text.trim();
                        match name.as_str() {
                            "pkgname" => {
                                if !text.is_empty() {
                                    comp.pkgname = text.to_string();
                                }
                            }
                            "id" => {
                                if in_provides {
                                    if !text.is_empty() {
                                        comp.provides_ids.push(text.to_string());
                                    }
                                } else if comp.id.is_empty() && !text.is_empty() {
                                    comp.id = text.to_string();
                                }
                            }
                            "launchable" => {
                                if !text.is_empty() {
                                    comp.launchables.push(text.to_string());
                                }
                            }
                            "name" => {
                                if is_default_lang && !text.is_empty() && comp.name.is_empty() {
                                    comp.name = text.to_string();
                                }
                            }
                            "summary" => {
                                if is_default_lang && !text.is_empty() && comp.summary.is_empty() {
                                    comp.summary = text.to_string();
                                }
                            }
                            "p" => {
                                if is_default_lang && !text.is_empty() {
                                    let prev = comp.description.get_or_insert_with(String::new);
                                    if !prev.is_empty() {
                                        prev.push_str("\n\n");
                                    }
                                    prev.push_str(text);
                                }
                            }
                            "developer_name" => {
                                if is_default_lang && !text.is_empty() {
                                    comp.developer = Some(text.to_string());
                                }
                            }
                            "project_license" => {
                                if !text.is_empty() {
                                    comp.license = Some(text.to_string());
                                }
                            }
                            "url" => {
                                if current_type == "homepage" && !text.is_empty() {
                                    comp.homepage = Some(text.to_string());
                                }
                            }
                            "category" => {
                                if let Some(cat) = parse_category(text) {
                                    if !comp.categories.contains(&cat) {
                                        comp.categories.push(cat);
                                    }
                                }
                            }
                            "keyword" => {
                                if !text.is_empty() {
                                    let kw_lower = text.to_lowercase();
                                    if !comp.keywords.contains(&kw_lower) {
                                        comp.keywords.push(kw_lower);
                                    }
                                }
                            }
                            "screenshot" | "image" => {
                                if (text.starts_with("http://") || text.starts_with("https://"))
                                    && (current_type == "source" || current_type.is_empty() || comp.screenshots.is_empty())
                                {
                                    if !comp.screenshots.contains(&text.to_string()) {
                                        comp.screenshots.push(text.to_string());
                                    }
                                }
                            }
                            "icon" => {
                                if !text.is_empty() {
                                    if current_type == "stock" {
                                        comp.icon = Some(PackageIcon::Themed(text.to_string()));
                                    } else if current_type == "cached" {
                                        let clean_icon = if let Some((_, icon_part)) = text.split_once('_') {
                                            icon_part.strip_suffix(".jxl")
                                                .or_else(|| icon_part.strip_suffix(".png"))
                                                .or_else(|| icon_part.strip_suffix(".svg"))
                                                .unwrap_or(icon_part)
                                        } else {
                                            text.strip_suffix(".jxl")
                                                .or_else(|| text.strip_suffix(".png"))
                                                .or_else(|| text.strip_suffix(".svg"))
                                                .unwrap_or(text)
                                        };
                                        comp.icon_cached_file = Some(text.to_string());
                                        comp.icon_theme_name = Some(clean_icon.to_string());
                                    } else if current_type == "remote" && (text.starts_with("http://") || text.starts_with("https://")) {
                                        if comp.icon.is_none() {
                                            comp.icon = Some(PackageIcon::RemoteUrl(text.to_string()));
                                        }
                                    } else if comp.icon.is_none() && !text.contains('/') {
                                        comp.icon = Some(PackageIcon::Themed(text.to_string()));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    current_tag.clear();
                    current_text.clear();
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
    }

    fn index_component(&mut self, mut comp: AlpmAppMetadata) {
        if comp.pkgname.is_empty() && !comp.id.is_empty() {
            let clean = comp.id.strip_suffix(".desktop").unwrap_or(&comp.id);
            if let Some(last) = clean.rsplit('.').next() {
                comp.pkgname = last.to_lowercase();
            } else {
                comp.pkgname = clean.to_lowercase();
            }
        }

        if !comp.pkgname.is_empty() {
            self.by_pkgname.insert(comp.pkgname.clone(), comp.clone());
            self.by_pkgname.insert(comp.pkgname.to_lowercase(), comp.clone());
        }

        if !comp.id.is_empty() {
            self.by_id.insert(comp.id.clone(), comp.clone());
            self.by_id.insert(comp.id.to_lowercase(), comp.clone());
            let clean = comp.id.strip_suffix(".desktop").unwrap_or(&comp.id);
            self.by_id.insert(clean.to_string(), comp.clone());
            self.by_id.insert(clean.to_lowercase(), comp.clone());
            if let Some(last) = clean.rsplit('.').next() {
                self.by_pkgname.entry(last.to_lowercase()).or_insert_with(|| comp.clone());
            }
        }

        for launch in &comp.launchables {
            self.by_id.insert(launch.clone(), comp.clone());
            self.by_id.insert(launch.to_lowercase(), comp.clone());
            let clean = launch.strip_suffix(".desktop").unwrap_or(launch);
            self.by_id.insert(clean.to_string(), comp.clone());
            self.by_id.insert(clean.to_lowercase(), comp.clone());
            if let Some(last) = clean.rsplit('.').next() {
                self.by_pkgname.entry(last.to_lowercase()).or_insert_with(|| comp.clone());
            }
        }

        for prov in &comp.provides_ids {
            self.by_id.insert(prov.clone(), comp.clone());
            self.by_id.insert(prov.to_lowercase(), comp.clone());
            let clean = prov.strip_suffix(".desktop").unwrap_or(prov);
            self.by_id.insert(clean.to_string(), comp.clone());
            self.by_id.insert(clean.to_lowercase(), comp.clone());
            if let Some(last) = clean.rsplit('.').next() {
                self.by_pkgname.entry(last.to_lowercase()).or_insert_with(|| comp.clone());
            }
        }
    }

    fn enrich_from_desktop_files(&mut self, dir: &Path) {
        if !dir.is_dir() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }

            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };

            let mut in_desktop_entry = false;
            let mut name = String::new();
            let mut comment = String::new();
            let mut icon = String::new();
            let mut categories_str = String::new();
            let mut nodisplay = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    in_desktop_entry = trimmed == "[Desktop Entry]";
                    continue;
                }
                if !in_desktop_entry {
                    continue;
                }

                if let Some((k, v)) = trimmed.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    match k {
                        "Name" if name.is_empty() => name = v.to_string(),
                        "Comment" if comment.is_empty() => comment = v.to_string(),
                        "Icon" if icon.is_empty() => icon = v.to_string(),
                        "Categories" => categories_str = v.to_string(),
                        "NoDisplay" if v.eq_ignore_ascii_case("true") => nodisplay = true,
                        _ => {}
                    }
                }
            }

            if nodisplay || name.is_empty() {
                continue;
            }

            let mut categories = Vec::new();
            for part in categories_str.split(';') {
                if let Some(cat) = parse_category(part.trim()) {
                    if !categories.contains(&cat) {
                        categories.push(cat);
                    }
                }
            }
            if categories.is_empty() {
                categories.push(PackageCategory::Utilities);
            }

            let icon_obj = if !icon.is_empty() {
                if icon.starts_with('/') && Path::new(&icon).is_file() {
                    Some(PackageIcon::LocalPath(icon))
                } else {
                    Some(PackageIcon::Themed(icon))
                }
            } else {
                Some(PackageIcon::Themed("package-x-generic".to_string()))
            };

            let meta = AlpmAppMetadata {
                pkgname: file_stem.to_string(),
                id: format!("{}.desktop", file_stem),
                name,
                summary: comment,
                description: None,
                icon: icon_obj,
                icon_cached_file: None,
                icon_theme_name: None,
                categories,
                screenshots: vec![],
                homepage: None,
                license: None,
                developer: None,
                launchables: vec![format!("{}.desktop", file_stem)],
                provides_ids: vec![],
                keywords: vec![],
            };

            self.by_pkgname.entry(file_stem.to_string()).or_insert_with(|| meta.clone());
            self.by_pkgname.entry(file_stem.to_lowercase()).or_insert_with(|| meta.clone());
            self.by_id.entry(format!("{}.desktop", file_stem)).or_insert_with(|| meta.clone());
            self.by_id.entry(file_stem.to_string()).or_insert(meta);
        }
    }
}

pub fn parse_category(cat: &str) -> Option<PackageCategory> {
    match cat.to_lowercase().as_str() {
        "development" | "programming" | "ide" | "building" | "debugger" => {
            Some(PackageCategory::Development)
        }
        "game" | "games" | "arcade" | "action" | "adventure" | "strategy" | "puzzle" => {
            Some(PackageCategory::Games)
        }
        "graphics" | "2dgraphics" | "rastergraphics" | "vectorgraphics" | "photography" => {
            Some(PackageCategory::Graphics)
        }
        "network" | "internet" | "webbrowser" | "email" | "chat" | "instantmessaging" => {
            Some(PackageCategory::Internet)
        }
        "audiovideo" | "audio" | "video" | "multimedia" | "music" | "player" | "recorder" => {
            Some(PackageCategory::Multimedia)
        }
        "office" | "wordprocessor" | "spreadsheet" | "presentation" | "finance" => {
            Some(PackageCategory::Office)
        }
        "system" | "settings" | "hardware" | "terminalemulator" | "packagemanager" => {
            Some(PackageCategory::System)
        }
        "utility" | "utilities" | "accessories" | "archiving" | "compression" | "filemanager" => {
            Some(PackageCategory::Utilities)
        }
        _ => None,
    }
}

fn get_cache_icon_dir() -> PathBuf {
    let cache_home = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".cache")
        });
    let dir = cache_home.join("pastor/icons");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn convert_jxl_to_png(jxl_path: &Path, png_path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let img = jxl_oxide::JxlImage::builder().open(jxl_path)?;
    let render = img.render_frame(0)?;
    let fb = render.image_all_channels();
    let (width, height, channels) = (fb.width(), fb.height(), fb.channels());
    let buf = fb.buf();
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let idx = (x + y * width) * channels;
            let r = (buf[idx].clamp(0.0, 1.0) * 255.0) as u8;
            let g = if channels > 1 { (buf[idx + 1].clamp(0.0, 1.0) * 255.0) as u8 } else { r };
            let b = if channels > 2 { (buf[idx + 2].clamp(0.0, 1.0) * 255.0) as u8 } else { r };
            let a = if channels > 3 { (buf[idx + 3].clamp(0.0, 1.0) * 255.0) as u8 } else { 255 };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    image::save_buffer(png_path, &rgba, width as u32, height as u32, image::ExtendedColorType::Rgba8)?;
    Ok(())
}

fn resolve_cached_icon(base: &Path, icon_file: &str) -> Option<String> {
    for repo_dir in &["archlinux-arch-extra", "archlinux-arch-multilib", "archlinux-arch-core"] {
        for size in &["128x128", "64x64", "48x48"] {
            let candidate = base.join(repo_dir).join(size).join(icon_file);
            if candidate.is_file() {
                if icon_file.ends_with(".png") || icon_file.ends_with(".svg") {
                    return Some(candidate.to_string_lossy().to_string());
                }
                if icon_file.ends_with(".jxl") {
                    let cache_dir = get_cache_icon_dir();
                    let stem = icon_file.strip_suffix(".jxl").unwrap_or(icon_file);
                    let png_candidate = cache_dir.join(format!("{stem}.png"));
                    if png_candidate.is_file() {
                        return Some(png_candidate.to_string_lossy().to_string());
                    }
                    if convert_jxl_to_png(&candidate, &png_candidate).is_ok() {
                        return Some(png_candidate.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    None
}
