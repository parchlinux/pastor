//! URI Scheme Router for Pastor (`pastor://` and `appstream://`)
//!
//! Handles untrusted URI input safely by treating all routes as navigation requests,
//! never executing arbitrary commands or skipping confirmation dialogs.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PastorRoute {
    /// Open package details and review screen for installation
    Install(String),
    /// Open package details screen
    Details(String),
    /// Search for packages matching query
    Search(String),
    /// Navigate to Updates view
    Updates,
    /// Navigate to Installed software view
    Installed,
    /// Navigate to Explore store view
    Explore,
    /// Navigate to Settings and repository configuration
    Settings,
    /// Navigate to Btrfs Snapshots view
    Snapshots,
    /// Navigate to Downgrade tool
    Downgrade,
}

/// Parses a URI into a validated `PastorRoute`.
///
/// Returns `None` if the URI is not recognized or contains invalid/dangerous characters.
pub fn parse_uri(raw_uri: &str) -> Option<PastorRoute> {
    let trimmed = raw_uri.trim();
    if trimmed.is_empty() {
        return None;
    }

    // AppStream scheme handling: appstream://<component-id> or appstream:<component-id>
    if let Some(target) = trimmed
        .strip_prefix("appstream://")
        .or_else(|| trimmed.strip_prefix("appstream:"))
    {
        let clean = clean_package_name(target);
        if is_valid_identifier(&clean) {
            return Some(PastorRoute::Details(clean));
        }
        return None;
    }

    // Pastor scheme handling: pastor://<action>/<arg> or pastor:<action>/<arg>
    let body = trimmed
        .strip_prefix("pastor://")
        .or_else(|| trimmed.strip_prefix("pastor:"))?;

    let (path_part, query_part) = match body.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (body, None),
    };

    let path_segments: Vec<&str> = path_part
        .split('/')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let action = path_segments.first().copied().unwrap_or("");

    match action {
        "install" => {
            let target = path_segments.get(1).copied().unwrap_or("");
            let decoded = url_decode(target);
            let clean = clean_package_name(&decoded);
            if is_valid_identifier(&clean) {
                Some(PastorRoute::Install(clean))
            } else {
                None
            }
        }
        "details" | "show" => {
            let target = path_segments.get(1).copied().unwrap_or("");
            let decoded = url_decode(target);
            let clean = clean_package_name(&decoded);
            if is_valid_identifier(&clean) {
                Some(PastorRoute::Details(clean))
            } else {
                None
            }
        }
        "search" => {
            let query = if let Some(q_param) = query_part.and_then(extract_search_param) {
                url_decode(&q_param)
            } else if let Some(path_q) = path_segments.get(1) {
                url_decode(path_q)
            } else {
                String::new()
            };
            let sanitized = sanitize_query(&query);
            if sanitized.is_empty() {
                Some(PastorRoute::Explore)
            } else {
                Some(PastorRoute::Search(sanitized))
            }
        }
        "updates" => Some(PastorRoute::Updates),
        "installed" => Some(PastorRoute::Installed),
        "explore" => Some(PastorRoute::Explore),
        "settings" => Some(PastorRoute::Settings),
        "snapshots" => Some(PastorRoute::Snapshots),
        "downgrade" => Some(PastorRoute::Downgrade),
        _ => None,
    }
}

fn clean_package_name(raw: &str) -> String {
    let clean = raw.trim().trim_matches('/');
    clean.strip_suffix(".desktop").unwrap_or(clean).to_string()
}

fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    // Prevent path traversal and special characters
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    name.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '+' || c == '@'
    })
}

fn sanitize_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.len() > 256 {
        trimmed[..256].to_string()
    } else {
        trimmed.to_string()
    }
}

fn extract_search_param(query_str: &str) -> Option<String> {
    for pair in query_str.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == "q" || k == "query" {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(val as char);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            result.push(' ');
            i += 1;
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_install_route() {
        assert_eq!(
            parse_uri("pastor://install/firefox"),
            Some(PastorRoute::Install("firefox".to_string()))
        );
        assert_eq!(
            parse_uri("pastor://install/visual-studio-code-bin"),
            Some(PastorRoute::Install("visual-studio-code-bin".to_string()))
        );
        assert_eq!(
            parse_uri("pastor:install/spotify"),
            Some(PastorRoute::Install("spotify".to_string()))
        );
    }

    #[test]
    fn test_parse_details_route() {
        assert_eq!(
            parse_uri("pastor://details/gnome-disk-utility"),
            Some(PastorRoute::Details("gnome-disk-utility".to_string()))
        );
        assert_eq!(
            parse_uri("pastor://show/gparted.desktop"),
            Some(PastorRoute::Details("gparted".to_string()))
        );
        assert_eq!(
            parse_uri("appstream://org.mozilla.firefox"),
            Some(PastorRoute::Details("org.mozilla.firefox".to_string()))
        );
        assert_eq!(
            parse_uri("appstream:vlc.desktop"),
            Some(PastorRoute::Details("vlc".to_string()))
        );
    }

    #[test]
    fn test_parse_navigation_routes() {
        assert_eq!(parse_uri("pastor://updates"), Some(PastorRoute::Updates));
        assert_eq!(parse_uri("pastor://installed"), Some(PastorRoute::Installed));
        assert_eq!(parse_uri("pastor://explore"), Some(PastorRoute::Explore));
        assert_eq!(parse_uri("pastor://settings"), Some(PastorRoute::Settings));
        assert_eq!(parse_uri("pastor://snapshots"), Some(PastorRoute::Snapshots));
        assert_eq!(parse_uri("pastor://downgrade"), Some(PastorRoute::Downgrade));
    }

    #[test]
    fn test_parse_search_route() {
        assert_eq!(
            parse_uri("pastor://search/disk%20utility"),
            Some(PastorRoute::Search("disk utility".to_string()))
        );
        assert_eq!(
            parse_uri("pastor://search?q=gparted"),
            Some(PastorRoute::Search("gparted".to_string()))
        );
        assert_eq!(
            parse_uri("pastor://search?query=text+editor"),
            Some(PastorRoute::Search("text editor".to_string()))
        );
    }

    #[test]
    fn test_reject_malicious_and_invalid_uris() {
        assert_eq!(parse_uri("pastor://install/../../etc/passwd"), None);
        assert_eq!(parse_uri("pastor://install/rm%20-rf"), None);
        assert_eq!(parse_uri("pastor://install/curl|bash"), None);
        assert_eq!(parse_uri("pastor://exec/somecommand"), None);
        assert_eq!(parse_uri("pastor://evil"), None);
        assert_eq!(parse_uri("http://example.com"), None);
        assert_eq!(parse_uri(""), None);
    }
}
