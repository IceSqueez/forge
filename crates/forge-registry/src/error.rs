#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("duplicate kind id: {0}")]
    DuplicateId(String),

    #[error("unknown kind id: {0}")]
    UnknownKindId(String),
}
