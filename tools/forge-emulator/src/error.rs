use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmulatorError {
    #[error("control host `{host}` is not a loopback address")]
    NonLoopbackHost { host: String },

    #[error("control port must be non-zero")]
    ZeroPort,

    #[error("bearer token missing: set {variable}")]
    MissingToken { variable: &'static str },

    #[error("control connection failed: {reason}")]
    Connect { reason: String },

    #[error("control connection timed out")]
    ConnectTimeout,

    #[error("control connection closed")]
    ConnectionClosed,

    #[error("`{request}` got no response in time")]
    RequestTimeout { request: &'static str },

    #[error("authentication refused: {message}")]
    AuthRefused { message: String },

    #[error("`{request}` refused: {message}")]
    Refused {
        request: &'static str,
        code: Option<String>,
        message: String,
    },

    #[error("`{request}` answered with an unexpected shape: {reason}")]
    UnexpectedResponse {
        request: &'static str,
        reason: String,
    },

    #[error("writing output failed: {reason}")]
    Output { reason: String },
}
