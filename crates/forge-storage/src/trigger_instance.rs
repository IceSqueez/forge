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
    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError>;
    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError>;
    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError>;
    async fn upsert_default(
        &self,
        kind_id: &str,
        name: &str,
    ) -> Result<TriggerInstanceId, StorageError>;
    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError>;
}
