use std::path::PathBuf;

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

    #[error("refusing to seed: {variable} must name the fixture directory")]
    DataDirUnset { variable: &'static str },

    #[error("refusing to seed: {variable} is set, so forge would not read the fixture's key file")]
    KeyFileOverride { variable: &'static str },

    #[error("fixture directory {} is unusable: {reason}", path.display())]
    DataDir { path: PathBuf, reason: String },

    #[error("fixture directory {} is not empty; seed a fresh directory", path.display())]
    DataDirNotEmpty { path: PathBuf },

    #[error("invalid fixture: {reason}")]
    InvalidFixture { reason: String },

    #[error("no free loopback port: {reason}")]
    PortProbe { reason: String },

    #[error("seeding storage failed: {reason}")]
    Storage { reason: String },

    #[error("seeder process failed: {reason}")]
    SeederProcess { reason: String },
}
