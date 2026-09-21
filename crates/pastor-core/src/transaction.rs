use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStep {
    Idle,
    CreatingSnapshot(String),
    SyncingDatabase,
    CheckingDependencies,
    Downloading {
        current_bytes: u64,
        total_bytes: u64,
        speed_bps: u64,
    },
    CheckingIntegrity,
    CheckingConflicts,
    ApplyingChanges {
        package: String,
        index: usize,
        total: usize,
    },
    RunningHooks(String),
    FinalizingSnapshot,
    Completed,
    Cancelled,
    Failed(String),
}

impl TransactionStep {
    pub fn description(&self) -> String {
        match self {
            Self::Idle => "Idle".to_string(),
            Self::CreatingSnapshot(desc) => format!("Creating Snapper snapshot: {desc}"),
            Self::SyncingDatabase => "Synchronizing package databases...".to_string(),
            Self::CheckingDependencies => "Checking dependencies...".to_string(),
            Self::Downloading { current_bytes, total_bytes, speed_bps } => {
                if *total_bytes > 100 {
                    let curr_str = crate::package::format_size(*current_bytes);
                    let tot_str = crate::package::format_size(*total_bytes);
                    let kb_speed = *speed_bps as f64 / 1024.0;
                    format!("Downloading ({} / {} at {:.0} KB/s)...", curr_str, tot_str, kb_speed)
                } else {
                    format!("Downloading ({}%)...", current_bytes)
                }
            }
            Self::CheckingIntegrity => "Verifying package signatures & integrity...".to_string(),
            Self::CheckingConflicts => "Checking file conflicts...".to_string(),
            Self::ApplyingChanges { package, index, total } => {
                format!("Installing {package} ({index}/{total})...")
            }
            Self::RunningHooks(hook) => format!("Running hook: {hook}"),
            Self::FinalizingSnapshot => "Finalizing Snapper post-snapshot...".to_string(),
            Self::Completed => "Transaction completed successfully!".to_string(),
            Self::Cancelled => "Transaction cancelled".to_string(),
            Self::Failed(err) => format!("Transaction failed: {err}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionEvent {
    pub step: TransactionStep,
    pub progress_fraction: f32,
    pub log_message: String,
}
