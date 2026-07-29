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

    #[error("overlay identity '{0}' is not a safe directory name")]
    UnsafeIdentity(String),

    #[error("'{path}' resolves outside the overlay root")]
    OutsideRoot { path: String },

    #[error("'{path}' is a symbolic link and will not be written through")]
    SymlinkedPath { path: String },

    #[error("could not build the config document: {0}")]
    ConfigDocument(#[from] serde_json::Error),

    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}
