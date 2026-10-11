use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error("storage backend error: {0}")]
    Backend(String),
}

pub type StorageResult<T> = Result<T, StorageError>;
