use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::PastorError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub number: u64,
    pub date: DateTime<Utc>,
    pub description: String,
    pub pre_number: Option<u64>,
    pub cleanup: String,
}

#[async_trait]
pub trait SnapshotBackend: Send + Sync {
    /// Create a pre-transaction snapshot (e.g. before updating or installing)
    async fn create_pre_snapshot(&self, description: &str) -> Result<u64, PastorError>;

    /// Create a post-transaction snapshot paired with the pre-snapshot
    async fn create_post_snapshot(&self, pre_number: u64, description: &str) -> Result<u64, PastorError>;

    /// List existing snapshots
    async fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>, PastorError>;

    /// Delete a specific snapshot
    async fn delete_snapshot(&self, number: u64) -> Result<(), PastorError>;
}

/// Fallback no-op snapshot backend when snapper is not available or inactive
#[derive(Debug, Default, Clone)]
pub struct NullSnapshotBackend;

#[async_trait]
impl SnapshotBackend for NullSnapshotBackend {
    async fn create_pre_snapshot(&self, _description: &str) -> Result<u64, PastorError> {
        Ok(0)
    }

    async fn create_post_snapshot(&self, _pre_number: u64, _description: &str) -> Result<u64, PastorError> {
        Ok(0)
    }

    async fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>, PastorError> {
        Ok(vec![])
    }

    async fn delete_snapshot(&self, _number: u64) -> Result<(), PastorError> {
        Ok(())
    }
}
