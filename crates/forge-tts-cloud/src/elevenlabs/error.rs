use thiserror::Error;

#[derive(Debug, Error)]
pub enum ElevenLabsError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("quota exhausted: {0}")]
    QuotaExceeded(String),
    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}
