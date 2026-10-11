use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("network error: {0}")]
    Network(String),
    #[error("{source_name} answered HTTP {status}")]
    Http {
        source_name: &'static str,
        status: u16,
    },
    #[error("rate limited, retry in {retry_after:?}")]
    RateLimited { retry_after: Duration },
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("license {0:?} is not allowed by the current policy")]
    LicenseNotAllowed(String),
    #[error("invalid download: {0}")]
    InvalidDownload(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for MediaError {
    fn from(e: std::io::Error) -> Self {
        MediaError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for MediaError {
    fn from(e: serde_json::Error) -> Self {
        MediaError::Parse(e.to_string())
    }
}
