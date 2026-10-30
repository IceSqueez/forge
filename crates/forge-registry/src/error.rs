use crate::config_messages::step_rejected;
use crate::config_problem::{ConfigProblem, ConfigReason};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("duplicate kind id: {0}")]
    DuplicateId(String),

    #[error("unknown kind id: {0}")]
    UnknownKindId(String),

    #[error("{0}")]
    InvalidConfig(ConfigProblem),

    #[error("sub-action nesting depth {0} exceeds the configured bound")]
    DepthExceeded(u32),

    #[error("remapped {namespace} reference is not a valid id: {value}")]
    InvalidRemap { namespace: String, value: String },

    #[error("runner initialization failed: {0}")]
    RunnerInit(String),
}

impl RegistryError {
    pub fn invalid_field(field: impl Into<String>, reason: ConfigReason) -> Self {
        Self::InvalidConfig(ConfigProblem::field(field, reason))
    }

    pub fn invalid_config(reason: ConfigReason) -> Self {
        Self::InvalidConfig(ConfigProblem::whole(reason))
    }

    pub fn into_config_problem(self) -> ConfigProblem {
        match self {
            Self::InvalidConfig(problem) => problem,
            _ => ConfigProblem::whole(step_rejected()),
        }
    }
}
