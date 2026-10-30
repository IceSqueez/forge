use std::collections::HashMap;

use async_trait::async_trait;
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId};

use crate::StorageError;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerChanges {
    pub removed: Vec<TriggerInstanceId>,
    pub unlinked: Vec<(ActionId, TriggerInstanceId)>,
    pub saved: Vec<TriggerInstance>,
    pub linked: Vec<(ActionId, TriggerInstanceId)>,
}

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
    async fn actions_by_instance(
        &self,
    ) -> Result<HashMap<TriggerInstanceId, Vec<ActionId>>, StorageError> {
        let mut links = HashMap::new();
        for instance in self.list_all().await? {
            let actions = self.actions_using(instance.id).await?;
            if !actions.is_empty() {
                links.insert(instance.id, actions);
            }
        }
        Ok(links)
    }

    async fn apply_changes(&self, changes: &TriggerChanges) -> Result<(), StorageError> {
        for instance_id in &changes.removed {
            for action_id in self.actions_using(*instance_id).await? {
                self.unlink_action(action_id, *instance_id).await?;
            }
            self.delete(*instance_id).await?;
        }
        for (action_id, instance_id) in &changes.unlinked {
            self.unlink_action(*action_id, *instance_id).await?;
        }
        for instance in &changes.saved {
            self.save(instance).await?;
        }
        for (action_id, instance_id) in &changes.linked {
            self.link_action(*action_id, *instance_id).await?;
        }
        Ok(())
    }

    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<(), StorageError>;
    async fn link_action_at(
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

    async fn toggle_enabled(&self, id: TriggerInstanceId) -> Result<Option<bool>, StorageError> {
        let Some(instance) = self.get(id).await? else {
            return Ok(None);
        };
        self.set_enabled(id, !instance.enabled).await?;
        Ok(Some(!instance.enabled))
    }

    async fn archive(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn restore(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(Vec::new())
    }
}
