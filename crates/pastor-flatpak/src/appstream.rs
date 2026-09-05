use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use pastor_core::{PackageCategory, PackageIcon};
use quick_xml::{events::Event, reader::Reader};

#[derive(Debug, Clone)]
pub struct AppstreamRelease {
    pub version: String,
    pub date: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppstreamComponent {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: Option<String>,
    pub developer_name: Option<String>,
    pub project_license: Option<String>,
    pub homepage: Option<String>,
    pub icon_filename: Option<String>,
    pub stock_icon: Option<String>,
    pub categories: Vec<PackageCategory>,
    pub releases: Vec<AppstreamRelease>,
    pub screenshots: Vec<String>,
    pub keywords: Vec<String>,
    pub dependencies: Vec<String>,
    pub bundle_ref: Option<String>,
    pub bundle_runtime: Option<String>,
    pub bundle_sdk: Option<String>,
    pub flatpak_id: Option<String>,
    pub remote_name: String,
}

impl AppstreamComponent {
    pub fn package_icon(&self, icons_base: Option<&Path>) -> Option<PackageIcon> {
        if let (Some(base), Some(icon_file)) = (icons_base, &self.icon_filename) {
            let path128 = base.join("128x128").join(icon_file);
            if path128.is_file() {
                return Some(PackageIcon::LocalPath(path128.to_string_lossy().to_string()));
            }
            let path64 = base.join("64x64").join(icon_file);
            if path64.is_file() {
                return Some(PackageIcon::LocalPath(path64.to_string_lossy().to_string()));
            }
        }

        if let Some(ref stock) = self.stock_icon {
            return Some(PackageIcon::Themed(stock.clone()));
        }

        Some(PackageIcon::Themed("package-x-generic".to_string()))
    }
}

#[derive(Debug, Default, Clone)]
pub struct AppstreamCatalog {
    pub components: HashMap<String, AppstreamComponent>,
    pub aliases: HashMap<String, String>,
    pub icons_dir: Option<PathBuf>,
}

impl AppstreamCatalog {
    pub fn get_component(&self, id: &str) -> Option<&AppstreamComponent> {
        if let Some(comp) = self.components.get(id) {
            return Some(comp);
        }
        if let Some(target) = self.aliases.get(id) {
            if let Some(comp) = self.components.get(target) {
                return Some(comp);
            }
        }
        if let Some(stripped) = id.strip_suffix(".desktop") {
            if let Some(comp) = self.components.get(stripped) {
                return Some(comp);
            }
            if let Some(target) = self.aliases.get(stripped) {
                if let Some(comp) = self.components.get(target) {
                    return Some(comp);
                }
            }
        }
        None
    }

    pub fn load_from_dir(appstream_dir: &Path, remote_name: &str) -> Self {
        let mut catalog = Self::default();

        // Check if there is an 'active' subfolder (standard flathub appstream layout)
        let effective_dir = if appstream_dir.join("active").is_dir() {
            appstream_dir.join("active")
        } else {
            appstream_dir.to_path_buf()
        };

        let icons_dir = effective_dir.join("icons");
        if icons_dir.is_dir() {
            catalog.icons_dir = Some(icons_dir);
        } else if appstream_dir.join("icons").is_dir() {
            catalog.icons_dir = Some(appstream_dir.join("icons"));
        }

        let xml_path = effective_dir.join("appstream.xml");
        let xml_gz_path = effective_dir.join("appstream.xml.gz");

        if xml_path.is_file() {
            if let Ok(file) = File::open(&xml_path) {
                let reader = BufReader::new(file);
                catalog.parse_xml(reader, remote_name);
            }
        } else if xml_gz_path.is_file() {
            if let Ok(file) = File::open(&xml_gz_path) {
                let gz = GzDecoder::new(BufReader::new(file));
                let reader = BufReader::new(gz);
                catalog.parse_xml(reader, remote_name);
            }
        } else {
            // Check appstream_dir directly as fallback
            let fallback_xml = appstream_dir.join("appstream.xml");
            let fallback_gz = appstream_dir.join("appstream.xml.gz");
            if fallback_xml.is_file() {
                if let Ok(file) = File::open(&fallback_xml) {
                    let reader = BufReader::new(file);
                    catalog.parse_xml(reader, remote_name);
                }
            } else if fallback_gz.is_file() {
                if let Ok(file) = File::open(&fallback_gz) {
                    let gz = GzDecoder::new(BufReader::new(file));
                    let reader = BufReader::new(gz);
                    catalog.parse_xml(reader, remote_name);
                }
            }
        }

        catalog
    }

    pub fn parse_xml<R: std::io::BufRead>(&mut self, buf_reader: R, remote_name: &str) {
        let mut reader = Reader::from_reader(buf_reader);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::with_capacity(4096);
        let mut in_component = false;
        let mut is_desktop_app = false;
        let mut current_tag = String::new();
        let mut current_icon_type = String::new();
        let mut current_image_type = String::new();
        let mut current_url_type = String::new();
        let mut in_ignored_description = false;
        let mut is_tag_ignored_locale = false;

        // Temporary builder fields
        let mut id = String::new();
        let mut name = String::new();
        let mut summary = String::new();
        let mut description = String::new();
        let mut developer_name = String::new();
        let mut project_license = String::new();
        let mut homepage = String::new();
        let mut icon_filename: Option<String> = None;
        let mut stock_icon: Option<String> = None;
        let mut categories = Vec::new();
        let mut releases: Vec<AppstreamRelease> = Vec::new();
        let mut screenshots = Vec::new();
        let mut keywords = Vec::new();
        let mut dependencies = Vec::new();
        let mut current_bundle_type = String::new();
        let mut current_bundle_runtime: Option<String> = None;
        let mut current_bundle_sdk: Option<String> = None;
        let mut current_bundle_ref = String::new();

        let mut in_release = false;
        let mut current_rel_ver = String::new();
        let mut current_rel_date: Option<String> = None;
        let mut current_rel_desc = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag_name == "component" {
                        in_component = true;
                        is_desktop_app = false;
                        in_ignored_description = false;
                        is_tag_ignored_locale = false;
                        in_release = false;
                        id.clear();
                        name.clear();
                        summary.clear();
                        description.clear();
                        developer_name.clear();
                        project_license.clear();
                        homepage.clear();
                        icon_filename = None;
                        stock_icon = None;
                        categories.clear();
                        releases.clear();
                        screenshots.clear();
                        keywords.clear();
                        dependencies.clear();
                        current_rel_ver.clear();
                        current_rel_date = None;
                        current_rel_desc.clear();

                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" {
                                let val = attr.value.as_ref();
                                if val == b"desktop-application"
                                    || val == b"desktop"
                                    || val == b"console-application"
                                {
                                    is_desktop_app = true;
                                }
                            }
                        }
                    } else if in_component {
                        current_tag = tag_name.clone();

                        // Check xml:lang attribute
                        let mut lang_val: Option<String> = None;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"xml:lang" {
                                lang_val = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }

                        // We accept default English (no lang or en / en_US)
                        let is_allowed_lang = match &lang_val {
                            None => true,
                            Some(l) => l.starts_with("en"),
                        };

                        if tag_name == "description" {
                            in_ignored_description = !is_allowed_lang;
                        } else {
                            is_tag_ignored_locale = !is_allowed_lang;
                        }

                        if tag_name == "bundle" {
                            current_bundle_type.clear();
                            current_bundle_runtime = None;
                            current_bundle_sdk = None;
                            current_bundle_ref.clear();
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"type" => {
                                        current_bundle_type =
                                            String::from_utf8_lossy(&attr.value).to_string();
                                    }
                                    b"runtime" => {
                                        let val = String::from_utf8_lossy(&attr.value).to_string();
                                        current_bundle_runtime = Some(val.clone());
                                        let dep = format!("Runtime: {}", val);
                                        if !dependencies.contains(&dep) {
                                            dependencies.push(dep);
                                        }
                                    }
                                    b"sdk" => {
                                        let val = String::from_utf8_lossy(&attr.value).to_string();
                                        current_bundle_sdk = Some(val.clone());
                                        let dep = format!("SDK: {}", val);
                                        if !dependencies.contains(&dep) {
                                            dependencies.push(dep);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        } else if tag_name == "icon" {
                            current_icon_type.clear();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"type" {
                                    current_icon_type =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        } else if tag_name == "image" {
                            current_image_type.clear();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"type" {
                                    current_image_type =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        } else if tag_name == "url" {
                            current_url_type.clear();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"type" {
                                    current_url_type =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                        } else if tag_name == "release" {
                            in_release = true;
                            current_rel_ver.clear();
                            current_rel_date = None;
                            current_rel_desc.clear();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"version" {
                                    current_rel_ver =
                                        String::from_utf8_lossy(&attr.value).to_string();
                                } else if attr.key.as_ref() == b"date"
                                    || attr.key.as_ref() == b"timestamp"
                                {
                                    current_rel_date =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) if in_component && is_desktop_app => {
                    if in_ignored_description || is_tag_ignored_locale {
                        // Skip foreign language text
                        continue;
                    }
                    if let Ok(text) = e.unescape() {
                        let text_val = text.trim();
                        if !text_val.is_empty() {
                            match current_tag.as_str() {
                                "id" if id.is_empty() => id = text_val.to_string(),
                                "name" if name.is_empty() => name = text_val.to_string(),
                                "summary" if summary.is_empty() => summary = text_val.to_string(),
                                "bundle" => {
                                    if current_bundle_type == "flatpak" && current_bundle_ref.is_empty() {
                                        current_bundle_ref = text_val.to_string();
                                    }
                                }
                                "p" => {
                                    if in_release {
                                        if !current_rel_desc.is_empty() {
                                            current_rel_desc.push('\n');
                                        }
                                        current_rel_desc.push_str(text_val);
                                    } else {
                                        if !description.is_empty() {
                                            description.push('\n');
                                        }
                                        description.push_str(text_val);
                                    }
                                }
                                "developer_name" if developer_name.is_empty() => {
                                    developer_name = text_val.to_string();
                                }
                                "project_license" if project_license.is_empty() => {
                                    project_license = text_val.to_string();
                                }
                                "url" if current_url_type == "homepage" && homepage.is_empty() => {
                                    homepage = text_val.to_string();
                                }
                                "image" => {
                                    if (current_image_type == "source" || current_image_type.is_empty())
                                        && text_val.starts_with("http")
                                        && !screenshots.contains(&text_val.to_string())
                                    {
                                        screenshots.push(text_val.to_string());
                                    }
                                }
                                "category" => {
                                    if let Some(cat) = map_category(text_val) {
                                        if !categories.contains(&cat) {
                                            categories.push(cat);
                                        }
                                    }
                                }
                                "keyword" => {
                                    let kw = text_val.to_lowercase();
                                    if !keywords.contains(&kw) {
                                        keywords.push(kw);
                                    }
                                }
                                "icon" => {
                                    if current_icon_type == "cached" && icon_filename.is_none() {
                                        icon_filename = Some(text_val.to_string());
                                    } else if current_icon_type == "stock" && stock_icon.is_none() {
                                        stock_icon = Some(text_val.to_string());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag_name == "description" {
                        in_ignored_description = false;
                    }
                    if tag_name == "release" {
                        if !current_rel_ver.is_empty() {
                            releases.push(AppstreamRelease {
                                version: current_rel_ver.clone(),
                                date: current_rel_date.clone(),
                                description: if current_rel_desc.is_empty() {
                                    None
                                } else {
                                    Some(current_rel_desc.clone())
                                },
                            });
                        }
                        in_release = false;
                        current_rel_ver.clear();
                        current_rel_date = None;
                        current_rel_desc.clear();
                    }
                    if tag_name == "component" {
                        if in_component && is_desktop_app && !id.is_empty() && !name.is_empty() {
                            if categories.is_empty() {
                                categories.push(PackageCategory::Utilities);
                            }
                            let (bundle_ref, flatpak_id) = if !current_bundle_ref.is_empty() {
                                let fid = current_bundle_ref.split('/').nth(1).map(|s| s.to_string());
                                (Some(current_bundle_ref.clone()), fid)
                            } else {
                                (None, None)
                            };
                            let comp = AppstreamComponent {
                                id: id.clone(),
                                name: name.clone(),
                                summary: summary.clone(),
                                description: if description.is_empty() {
                                    None
                                } else {
                                    Some(description.clone())
                                },
                                developer_name: if developer_name.is_empty() {
                                    None
                                } else {
                                    Some(developer_name.clone())
                                },
                                project_license: if project_license.is_empty() {
                                    None
                                } else {
                                    Some(project_license.clone())
                                },
                                homepage: if homepage.is_empty() {
                                    None
                                } else {
                                    Some(homepage.clone())
                                },
                                icon_filename: icon_filename.clone(),
                                stock_icon: stock_icon.clone(),
                                categories: categories.clone(),
                                releases: releases.clone(),
                                screenshots: screenshots.clone(),
                                keywords: keywords.clone(),
                                dependencies: dependencies.clone(),
                                bundle_ref,
                                bundle_runtime: current_bundle_runtime.clone(),
                                bundle_sdk: current_bundle_sdk.clone(),
                                flatpak_id: flatpak_id.clone(),
                                remote_name: remote_name.to_string(),
                            };
                            let canonical_id = if let Some(ref fid) = flatpak_id {
                                fid.clone()
                            } else if id.ends_with(".desktop") {
                                id.strip_suffix(".desktop").unwrap().to_string()
                            } else {
                                id.clone()
                            };

                            // Only overwrite if existing is empty or if this remote is preferred
                            self.components.insert(canonical_id.clone(), comp.clone());

                            if id != canonical_id {
                                self.aliases.insert(id.clone(), canonical_id.clone());
                            }
                            if let Some(ref fid) = flatpak_id {
                                if fid != &canonical_id {
                                    self.aliases.insert(fid.clone(), canonical_id.clone());
                                }
                            }
                            if id.ends_with(".desktop") {
                                let stripped = id.strip_suffix(".desktop").unwrap();
                                if stripped != canonical_id {
                                    self.aliases.insert(stripped.to_string(), canonical_id.clone());
                                }
                            }
                        }
                        in_component = false;
                        is_desktop_app = false;
                        in_ignored_description = false;
                    }
                    is_tag_ignored_locale = false;
                    current_tag.clear();
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    tracing::warn!("XML parse error in AppStream: {e}");
                    break;
                }
                _ => {}
            }
            buf.clear();
        }
    }
}

fn map_category(cat_str: &str) -> Option<PackageCategory> {
    match cat_str {
        "Development" | "IDE" | "Debugger" | "Building" | "Translation" | "RevisionControl"
        | "WebDevelopment" => Some(PackageCategory::Development),
        "Game" | "ActionGame" | "AdventureGame" | "ArcadeGame" | "BoardGame" | "BlocksGame"
        | "CardGame" | "KidsGame" | "LogicGame" | "RolePlaying" | "Shooter" | "Simulation"
        | "SportsGame" | "StrategyGame" | "Emulator" => Some(PackageCategory::Games),
        "Graphics" | "2DGraphics" | "VectorGraphics" | "RasterGraphics" | "3DGraphics"
        | "Photography" | "Viewer" | "Scanning" | "OCR" => Some(PackageCategory::Graphics),
        "Network" | "WebBrowser" | "Email" | "Chat" | "InstantMessaging" | "Feed"
        | "FileTransfer" | "P2P" | "IRCClient" | "Telephony" | "News" | "RemoteAccess" => {
            Some(PackageCategory::Internet)
        }
        "AudioVideo" | "Audio" | "Video" | "Player" | "Recorder" | "Music" | "Midi"
        | "AudioVideoEditing" | "Mixer" | "Sequencer" | "Tuner" | "DiscBurning" => {
            Some(PackageCategory::Multimedia)
        }
        "Office" | "WordProcessor" | "Spreadsheet" | "Presentation" | "Publishing" | "Finance"
        | "Calendar" | "ContactManagement" | "Chart" | "FlowChart" => Some(PackageCategory::Office),
        "System" | "Settings" | "PackageManager" | "Monitor" | "Security" | "Filesystem"
        | "HardwareSettings" => Some(PackageCategory::System),
        "Utility" | "TextEditor" | "TerminalEmulator" | "Archiving" | "Calculator" | "Clock"
        | "FileTools" | "Accessibility" | "Education" | "Science" | "Math" | "Engineering"
        | "Documentation" | "Maps" => Some(PackageCategory::Utilities),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_desktop_component_firefox() {
        let sample_xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <components version="0.8" origin="flathub">
          <component type="desktop">
            <id>org.mozilla.firefox</id>
            <name>Firefox</name>
            <summary>Fast, Private &amp; Safe Web Browser</summary>
            <developer_name>Mozilla</developer_name>
            <description><p>Web browser</p></description>
            <icon height="128" type="cached" width="128">org.mozilla.firefox.png</icon>
            <categories>
              <category>Network</category>
              <category>WebBrowser</category>
            </categories>
            <keywords>
              <keyword>Browser</keyword>
              <keyword>Internet</keyword>
            </keywords>
          </component>
        </components>"#;

        let mut catalog = AppstreamCatalog::default();
        catalog.parse_xml(Cursor::new(sample_xml.as_bytes()), "flathub");

        assert_eq!(catalog.components.len(), 1);
        let firefox = catalog.components.get("org.mozilla.firefox").expect("Firefox parsed");
        assert_eq!(firefox.name, "Firefox");
        assert_eq!(firefox.summary, "Fast, Private & Safe Web Browser");
        assert_eq!(firefox.developer_name.as_deref(), Some("Mozilla"));
        assert!(firefox.categories.contains(&PackageCategory::Internet));
        assert!(firefox.keywords.contains(&"browser".to_string()));
        assert!(firefox.keywords.contains(&"internet".to_string()));
        assert_eq!(firefox.icon_filename.as_deref(), Some("org.mozilla.firefox.png"));
    }

    #[test]
    fn test_map_category_coverage() {
        assert_eq!(map_category("Development"), Some(PackageCategory::Development));
        assert_eq!(map_category("WebBrowser"), Some(PackageCategory::Internet));
        assert_eq!(map_category("InstantMessaging"), Some(PackageCategory::Internet));
        assert_eq!(map_category("AudioVideo"), Some(PackageCategory::Multimedia));
        assert_eq!(map_category("Music"), Some(PackageCategory::Multimedia));
        assert_eq!(map_category("Finance"), Some(PackageCategory::Office));
        assert_eq!(map_category("Science"), Some(PackageCategory::Utilities));
        assert_eq!(map_category("Education"), Some(PackageCategory::Utilities));
    }
}
