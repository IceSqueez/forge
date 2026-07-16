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

    /// Marks `id` archived: invisible to `get`, `list_all`, `list_user_defined`, and
    /// `list_for_action` until [`Self::restore`] is called. The row and its links in
    /// `action_trigger_instances` survive untouched - this is a soft delete, not
    /// [`Self::delete`]. Returns `false` when `id` does not exist or is already
    /// archived.
    ///
    /// The default impl has no generic representation of archived state without a
    /// dedicated column, so it returns [`StorageError::NotReady`]; a real backend
    /// must override this.
    async fn archive(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Clears the marker set by [`Self::archive`], restoring `id` to `get`/`list_all`/
    /// `list_user_defined`/`list_for_action` visibility. Returns `false` when `id`
    /// does not exist or is not currently archived.
    ///
    /// See [`Self::archive`] for the default-impl caveat.
    async fn restore(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Returns archived trigger instances only - the mirror of `list_all`, which
    /// excludes them.
    ///
    /// The default impl reports no archived entries, consistent with a backend that
    /// does not support archiving.
    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(Vec::new())
    }
}
