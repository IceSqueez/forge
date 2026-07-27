#[derive(Debug, thiserror::Error)]
pub enum VTubeError {
    #[error("connection failed: {0}")]
    Connect(String),

    #[error("VTube Studio API is disabled in VTS settings")]
    ApiDisabled,

    #[error("authentication popup was denied by the user")]
    TokenDenied,

    #[error("token request timed out waiting for popup acceptance")]
    TokenTimeout,

    #[error("stored token was rejected; re-authorization required")]
    TokenRejected,

    #[error("request failed: {message}")]
    Request { message: String },

    #[error("not connected")]
    NotConnected,

    #[error("connection timed out")]
    Timeout,

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
