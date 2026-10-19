#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("duplicate kind id: {0}")]
    DuplicateId(String),

    #[error("unknown kind id: {0}")]
    UnknownKindId(String),

    #[error("{0}")]
    InvalidConfig(String),

    #[error("sub-action nesting depth {0} exceeds the configured bound")]
    DepthExceeded(u32),

    #[error("remapped {namespace} reference is not a valid id: {value}")]
    InvalidRemap { namespace: String, value: String },

    #[error("runner initialization failed: {0}")]
    RunnerInit(String),
}
