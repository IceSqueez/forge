use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use forge_types::{Action, ActionId, Queue, QueueId, TriggerInstance, TriggerInstanceId};
use time::OffsetDateTime;

use crate::{
    ActionRepo, ActionTelemetry, ExecutionStatus, QueueRepo, StorageError, TriggerInstanceRepo,
};

/// Advances once per catalog write after the backend returns; an unchanged value means no newer write completed.
#[derive(Debug, Clone, Default)]
pub struct CatalogRevision(Arc<AtomicU64>);

impl CatalogRevision {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    pub fn advance(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    async fn after<T>(&self, write: impl Future<Output = T>) -> T {
        let _advance_on_exit = AdvanceOnDrop(self);
        write.await
    }
}

/// Advances even when the write future is dropped mid-flight, since the backend may still commit it.
struct AdvanceOnDrop<'a>(&'a CatalogRevision);

impl Drop for AdvanceOnDrop<'_> {
    fn drop(&mut self) {
        self.0.advance();
    }
}

pub struct RevisingActionRepo {
    inner: Arc<dyn ActionRepo>,
    revision: CatalogRevision,
}

impl RevisingActionRepo {
    pub fn wrap(inner: Arc<dyn ActionRepo>, revision: CatalogRevision) -> Arc<dyn ActionRepo> {
        Arc::new(Self { inner, revision })
    }
}

#[async_trait]
impl ActionRepo for RevisingActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        self.inner.list().await
    }

    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, action: &Action) -> Result<(), StorageError> {
        self.revision.after(self.inner.save(action)).await
    }

    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.delete(id)).await
    }

    async fn list_by_group<'a>(
        &'a self,
        group: Option<&'a str>,
    ) -> Result<Vec<Action>, StorageError> {
        self.inner.list_by_group(group).await
    }

    async fn telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError> {
        self.inner.telemetry(id).await
    }

    async fn record_execution(
        &self,
        action_id: ActionId,
        started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        self.inner
            .record_execution(action_id, started_at, duration_ms, status)
            .await
    }

    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        self.inner.prune_executions_before(cutoff).await
    }

    async fn set_enabled(&self, id: ActionId, enabled: bool) -> Result<bool, StorageError> {
        self.revision
            .after(self.inner.set_enabled(id, enabled))
            .await
    }

    async fn toggle_enabled(&self, id: ActionId) -> Result<Option<bool>, StorageError> {
        self.revision.after(self.inner.toggle_enabled(id)).await
    }

    async fn duplicate(
        &self,
        source_id: ActionId,
        new_id: ActionId,
        new_name: &str,
    ) -> Result<(), StorageError> {
        self.revision
            .after(self.inner.duplicate(source_id, new_id, new_name))
            .await
    }

    async fn archive(&self, id: ActionId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.archive(id)).await
    }

    async fn restore(&self, id: ActionId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.restore(id)).await
    }

    async fn list_archived(&self) -> Result<Vec<Action>, StorageError> {
        self.inner.list_archived().await
    }
}

pub struct RevisingTriggerInstanceRepo {
    inner: Arc<dyn TriggerInstanceRepo>,
    revision: CatalogRevision,
}

impl RevisingTriggerInstanceRepo {
    pub fn wrap(
        inner: Arc<dyn TriggerInstanceRepo>,
        revision: CatalogRevision,
    ) -> Arc<dyn TriggerInstanceRepo> {
        Arc::new(Self { inner, revision })
    }
}

#[async_trait]
impl TriggerInstanceRepo for RevisingTriggerInstanceRepo {
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_all().await
    }

    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_user_defined().await
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_for_action(action_id).await
    }

    async fn actions_using(
        &self,
        instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError> {
        self.inner.actions_using(instance_id).await
    }

    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        position: i64,
    ) -> Result<(), StorageError> {
        self.revision
            .after(self.inner.link_action(action_id, instance_id, position))
            .await
    }

    async fn unlink_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError> {
        self.revision
            .after(self.inner.unlink_action(action_id, instance_id))
            .await
    }

    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
        self.revision.after(self.inner.save(instance)).await
    }

    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.delete(id)).await
    }

    async fn upsert_default(
        &self,
        kind_id: &str,
        name: &str,
    ) -> Result<TriggerInstanceId, StorageError> {
        self.revision
            .after(self.inner.upsert_default(kind_id, name))
            .await
    }

    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError> {
        self.revision
            .after(self.inner.set_enabled(id, enabled))
            .await
    }

    async fn archive(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.archive(id)).await
    }

    async fn restore(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.restore(id)).await
    }

    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_archived().await
    }
}

pub struct RevisingQueueRepo {
    inner: Arc<dyn QueueRepo>,
    revision: CatalogRevision,
}

impl RevisingQueueRepo {
    pub fn wrap(inner: Arc<dyn QueueRepo>, revision: CatalogRevision) -> Arc<dyn QueueRepo> {
        Arc::new(Self { inner, revision })
    }
}

#[async_trait]
impl QueueRepo for RevisingQueueRepo {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        self.inner.list().await
    }

    async fn get(&self, id: QueueId) -> Result<Option<Queue>, StorageError> {
        self.inner.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<Queue>, StorageError> {
        self.inner.get_by_name(name).await
    }

    async fn save(&self, queue: &Queue) -> Result<(), StorageError> {
        self.revision.after(self.inner.save(queue)).await
    }

    async fn delete(&self, id: QueueId) -> Result<bool, StorageError> {
        self.revision.after(self.inner.delete(id)).await
    }
}

