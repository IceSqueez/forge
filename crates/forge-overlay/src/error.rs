#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("duplicate overlay type: {0}")]
    DuplicateKind(String),

    #[error("unknown overlay type: {0}")]
    UnknownKind(String),

    #[error("'{key}' expects {expected}")]
    WrongType { key: String, expected: &'static str },

    #[error("'{key}' does not accept the value '{value}'")]
    UnknownChoice { key: String, value: String },

    #[error("'{key}' must be between {min} and {max}")]
    OutOfRange { key: String, min: i64, max: i64 },
}
