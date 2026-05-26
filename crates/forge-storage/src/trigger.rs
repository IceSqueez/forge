use async_trait::async_trait;
use forge_types::{ActionId, Trigger, TriggerId};

use crate::StorageError;

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait TriggerRepo: Send + Sync {
    async fn list_for_action(&self, action_id: ActionId) -> Result<Vec<Trigger>, StorageError>;
    async fn save(&self, trigger: &Trigger) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: TriggerId) -> Result<bool, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn TriggerRepo) {}
}
