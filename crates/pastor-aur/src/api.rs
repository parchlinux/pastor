use pastor_core::{
    error::PastorError,
    package::{Package, PackageCategory, PackageIcon, PackageId, PackageSource, PackageState},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AurRpcPackage {
    pub id: Option<u64>,
    pub name: String,
    pub package_base: Option<String>,
    pub version: String,
    pub description: Option<String>,
    #[serde(rename = "URL")]
    pub url: Option<String>,
    pub num_votes: Option<u32>,
    pub popularity: Option<f64>,
    pub out_of_date: Option<i64>,
    pub maintainer: Option<String>,
    pub first_submitted: Option<i64>,
    pub last_modified: Option<i64>,
    pub depends: Option<Vec<String>>,
    pub make_depends: Option<Vec<String>>,
    pub check_depends: Option<Vec<String>>,
    pub license: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AurRpcResponse {
    pub version: u32,
    #[serde(rename = "type")]
    pub response_type: String,
    pub resultcount: usize,
    pub results: Vec<AurRpcPackage>,
}

pub struct AurClient {
    base_url: String,
    agent: ureq::Agent,
}

impl Default for AurClient {
    fn default() -> Self {
        let agent = ureq::AgentBuilder::new()
            .try_proxy_from_env(true)
            .timeout(std::time::Duration::from_secs(15))
            .build();
        Self {
            base_url: "https://aur.archlinux.org/rpc/v5".to_string(),
            agent,
        }
    }
}

impl AurClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn search(&self, query: &str) -> Result<Vec<AurRpcPackage>, PastorError> {
        let q = query.trim().to_string();
        if q.len() < 2 {
            return Ok(vec![]);
        }

        let agent = self.agent.clone();
        let url = format!("{}/search/{}", self.base_url, urlencoding_encode(&q));
        tokio::task::spawn_blocking(move || {
            let resp = agent
                .get(&url)
                .timeout(std::time::Duration::from_millis(1800))
                .call()
                .map_err(|e| PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("AUR RPC search request failed: {e}"),
                })?;

            let parsed: AurRpcResponse = resp.into_json().map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Failed to parse AUR RPC search response: {e}"),
            })?;

            let mut results = parsed.results;
            results.sort_by(|a, b| {
                let pop_b = b.popularity.unwrap_or(0.0);
                let pop_a = a.popularity.unwrap_or(0.0);
                pop_b.partial_cmp(&pop_a).unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.num_votes.unwrap_or(0).cmp(&a.num_votes.unwrap_or(0)))
            });

            Ok(results)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("AUR search task error: {e}"),
        })?
    }

    pub async fn info(&self, package_name: &str) -> Result<Option<AurRpcPackage>, PastorError> {
        let name = package_name.trim().to_string();
        if name.is_empty() {
            return Ok(None);
        }

        let agent = self.agent.clone();
        let url = format!("{}/info/{}", self.base_url, urlencoding_encode(&name));
        tokio::task::spawn_blocking(move || {
            let resp = agent
                .get(&url)
                .timeout(std::time::Duration::from_secs(10))
                .call()
                .map_err(|e| PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("AUR RPC info request failed: {e}"),
                })?;

            let parsed: AurRpcResponse = resp.into_json().map_err(|e| PastorError::BackendError {
                backend: "aur".into(),
                message: format!("Failed to parse AUR RPC info response: {e}"),
            })?;

            Ok(parsed.results.into_iter().next())
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("AUR info task error: {e}"),
        })?
    }

    pub async fn multi_info(&self, package_names: &[String]) -> Result<Vec<AurRpcPackage>, PastorError> {
        if package_names.is_empty() {
            return Ok(vec![]);
        }

        let agent = self.agent.clone();
        let base_url = self.base_url.clone();
        let names: Vec<String> = package_names.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut all_results = Vec::new();

            for chunk in names.chunks(50) {
                let mut url = format!("{}/info?", base_url);
                for (i, name) in chunk.iter().enumerate() {
                    if i > 0 {
                        url.push('&');
                    }
                    url.push_str("arg[]=");
                    url.push_str(&urlencoding_encode(name));
                }

                let resp = agent
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(15))
                    .call()
                    .map_err(|e| PastorError::BackendError {
                        backend: "aur".into(),
                        message: format!("AUR RPC multiinfo request failed: {e}"),
                    })?;

                let parsed: AurRpcResponse = resp.into_json().map_err(|e| PastorError::BackendError {
                    backend: "aur".into(),
                    message: format!("Failed to parse AUR RPC multiinfo response: {e}"),
                })?;

                all_results.extend(parsed.results);
            }

            Ok(all_results)
        })
        .await
        .map_err(|e| PastorError::BackendError {
            backend: "aur".into(),
            message: format!("AUR multiinfo task error: {e}"),
        })?
    }
}

pub fn rpc_pkg_to_package(rpc: AurRpcPackage, installed_ver: Option<String>) -> Package {
    let state = if let Some(ref inst) = installed_ver {
        if alpm_vercmp(&rpc.version, inst) > 0 {
            PackageState::UpdateAvailable
        } else {
            PackageState::Installed
        }
    } else {
        PackageState::NotInstalled
    };

    let license_str = rpc.license.as_ref().map(|l| l.join(", "));
    let mut deps = Vec::new();
    if let Some(d) = rpc.depends {
        deps.extend(d);
    }
    if let Some(m) = rpc.make_depends {
        deps.extend(m);
    }

    Package {
        id: PackageId::new(rpc.name.clone(), PackageSource::Aur),
        name: rpc.name.clone(),
        display_name: Some(rpc.name.clone()),
        version: rpc.version,
        installed_version: installed_ver,
        summary: rpc.description.unwrap_or_else(|| "AUR Community Package".to_string()),
        description: None,
        icon: Some(PackageIcon::Themed("package-x-generic".to_string())),
        screenshots: Vec::new(),
        homepage: rpc.url,
        license: license_str,
        maintainer: rpc.maintainer.or_else(|| Some("Orphan / None".to_string())),
        categories: vec![PackageCategory::Utilities],
        size_installed: None,
        size_download: None,
        dependencies: deps,
        changelog: None,
        state,
    }
}

fn urlencoding_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

pub fn alpm_vercmp(v1: &str, v2: &str) -> i32 {
    match pastor_alpm::vercmp(v1, v2) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Less => -1,
    }
}
