use forge_types::MessageRef;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartFailureCategory {
    NotConfigured,
    Credentials,
    Unreachable,
    RateLimited,
    Refused,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Error)]
#[error("integration start failed ({category:?}): {}", message.code())]
pub struct IntegrationStartError {
    category: StartFailureCategory,
    message: MessageRef,
}

impl IntegrationStartError {
    pub fn new(category: StartFailureCategory, message: MessageRef) -> Self {
        Self { category, message }
    }

    pub fn category(&self) -> StartFailureCategory {
        self.category
    }

    pub fn message(&self) -> &MessageRef {
        &self.message
    }
}
