use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_types::{Action, ActionId, Queue, TriggerInstance, TriggerInstanceId};

use crate::{ActionRepo, QueueRepo, StorageError};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerChanges {
    pub removed: Vec<TriggerInstanceId>,
    pub unlinked: Vec<(ActionId, TriggerInstanceId)>,
    pub saved: Vec<TriggerInstance>,
    pub linked: Vec<(ActionId, TriggerInstanceId)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerWiring {
    pub created_queue: Option<Queue>,
    pub action: Action,
    pub instance: TriggerInstance,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait TriggerInstanceRepo: Send + Sync {
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError>;

    async fn count_of_kinds(&self, kind_ids: &[String]) -> Result<usize, StorageError>;

    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError>;
    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError>;
    async fn instances_by_action(
        &self,
    ) -> Result<HashMap<ActionId, Vec<TriggerInstance>>, StorageError>;
    async fn actions_using(
        &self,
        instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError>;
    async fn actions_by_instance(
        &self,
    ) -> Result<HashMap<TriggerInstanceId, Vec<ActionId>>, StorageError>;

    async fn apply_changes(&self, changes: &TriggerChanges) -> Result<(), StorageError>;

    async fn commit_overlay_wiring(
        &self,
        wiring: &TriggerWiring,
        queues: Arc<dyn QueueRepo>,
        actions: Arc<dyn ActionRepo>,
    ) -> Result<(), StorageError>;

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
    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool)
    -> Result<bool, StorageError>;

    async fn set_all_enabled(
        &self,
        ids: &[TriggerInstanceId],
        enabled: bool,
    ) -> Result<(), StorageError>;

    async fn toggle_enabled(&self, id: TriggerInstanceId) -> Result<Option<bool>, StorageError>;

    async fn archive(&self, id: TriggerInstanceId) -> Result<bool, StorageError>;

    async fn restore(&self, id: TriggerInstanceId) -> Result<bool, StorageError>;

    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError>;
}
