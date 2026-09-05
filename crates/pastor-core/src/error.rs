use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum PastorError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Backend error ({backend}): {message}")]
    BackendError {
        backend: String,
        message: String,
    },

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Snapshot failed ({provider}): {message}")]
    SnapshotFailed {
        provider: String,
        message: String,
    },

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}
