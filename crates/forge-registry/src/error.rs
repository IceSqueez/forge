#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("duplicate kind id: {0}")]
    DuplicateId(String),

    #[error("unknown kind id: {0}")]
    UnknownKindId(String),

    #[error("sub-action nesting depth {0} exceeds the configured bound")]
    DepthExceeded(u32),

    #[error("runner initialization failed: {0}")]
    RunnerInit(String),
}
