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

    #[error("VTube Studio rejected the request: {message} (errorID={error_id})")]
    Rejected { error_id: i64, message: String },

    #[error("not connected")]
    NotConnected,

    #[error("VTube Studio did not answer in time")]
    Timeout,

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
