use crate::state::AurStateStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustStatus {
    FirstInstall,
    MaintainerChanged { previous: String, current: String },
    UnchangedMaintainer { maintainer: String },
    Orphaned { previous: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustReport {
    pub status: TrustStatus,
    pub is_review_required: bool,
    pub title: String,
    pub details: String,
    pub badge_class: String,
}

impl TrustReport {
    pub fn assess(
        pkg_name: &str,
        current_maintainer: Option<&str>,
        state_store: &AurStateStore,
    ) -> Self {
        let stored_record = state_store.get_record(pkg_name);
        let current_m = current_maintainer.unwrap_or("").trim();

        match (stored_record, current_m) {
            (None, "") => Self {
                status: TrustStatus::Orphaned { previous: None },
                is_review_required: true,
                title: "Orphaned Package (First Install)".to_string(),
                details: format!(
                    "Package '{}' has no active maintainer on the AUR. Full source review required.",
                    pkg_name
                ),
                badge_class: "warning".to_string(),
            },
            (None, m) => Self {
                status: TrustStatus::FirstInstall,
                is_review_required: true,
                title: "First Install: New Maintainer".to_string(),
                details: format!(
                    "First time installing '{}' maintained by '{}'. Review PKGBUILD before building.",
                    pkg_name, m
                ),
                badge_class: "accent".to_string(),
            },
            (Some(rec), "") => Self {
                status: TrustStatus::Orphaned {
                    previous: rec.last_maintainer.clone(),
                },
                is_review_required: true,
                title: "⚠️ Package Orphaned!".to_string(),
                details: format!(
                    "Package '{}' was previously maintained by '{}' but is now orphaned! High security risk.",
                    pkg_name,
                    rec.last_maintainer.as_deref().unwrap_or("unknown")
                ),
                badge_class: "destructive".to_string(),
            },
            (Some(rec), m) => {
                let prev_m = rec.last_maintainer.as_deref().unwrap_or("");
                if prev_m == m {
                    Self {
                        status: TrustStatus::UnchangedMaintainer {
                            maintainer: m.to_string(),
                        },
                        is_review_required: false,
                        title: "Known Maintainer".to_string(),
                        details: format!(
                            "Maintained by trusted author '{}' (unchanged since last install).",
                            m
                        ),
                        badge_class: "success".to_string(),
                    }
                } else {
                    Self {
                        status: TrustStatus::MaintainerChanged {
                            previous: prev_m.to_string(),
                            current: m.to_string(),
                        },
                        is_review_required: true,
                        title: "⚠️ Maintainer Changed!".to_string(),
                        details: format!(
                            "Package maintainer changed from '{}' to '{}'! This matches known 2026 supply chain hijacking patterns.",
                            if prev_m.is_empty() { "none" } else { prev_m },
                            m
                        ),
                        badge_class: "destructive".to_string(),
                    }
                }
            }
        }
    }
}
