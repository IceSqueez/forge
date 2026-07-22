use async_trait::async_trait;
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId};

use crate::StorageError;

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait TriggerInstanceRepo: Send + Sync {
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError>;
    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError>;
    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError>;
    async fn actions_using(
        &self,
        instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError>;
    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        position: i64,
    ) -> Result<(), StorageError>;
    async fn unlink_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError>;
    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError>;
    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError>;
    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError>;
    async fn upsert_default(
        &self,
        kind_id: &str,
        name: &str,
    ) -> Result<TriggerInstanceId, StorageError>;
    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError>;

    /// Soft delete (not [`Self::delete`]): hides `id` from `get`/`list_all`/
    /// `list_user_defined`/`list_for_action` until [`Self::restore`]; the row and its
    /// `action_trigger_instances` links survive untouched. Default impl has no generic
    /// archived-state representation and returns [`StorageError::NotReady`].
    async fn archive(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Reverses [`Self::archive`]; see its default-impl caveat.
    async fn restore(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Mirror of `list_all`, which excludes archived entries. Default impl reports none.
    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(Vec::new())
    }
}
