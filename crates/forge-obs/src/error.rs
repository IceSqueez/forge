#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("connection failed: {0}")]
    Connect(String),

    #[error("authentication rejected")]
    Authentication,

    #[error("not connected to OBS")]
    Disconnected,

    #[error("request timed out")]
    Timeout,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("raw payload serialization: {0}")]
    Payload(#[from] serde_json::Error),

    #[error("request failed: {request_type} - {message}")]
    Request {
        request_type: String,
        message: String,
    },
}

pub(crate) fn map_request_error(request_type: &str, e: obws::error::Error) -> ObsError {
    match e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected => ObsError::Disconnected,
        _ => ObsError::Request {
            request_type: request_type.to_owned(),
            message: e.to_string(),
        },
    }
}
