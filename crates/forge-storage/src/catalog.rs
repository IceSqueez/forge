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

    /// The write runs detached from its caller, so a cancelled caller cannot hide a commit that lands later.
    async fn after<T, W>(&self, write: W) -> Result<T, StorageError>
    where
        T: Send + 'static,
        W: Future<Output = Result<T, StorageError>> + Send + 'static,
    {
        let advance_on_exit = AdvanceOnDrop(self.clone());
        let landed = tokio::spawn(async move {
            let _advance_on_exit = advance_on_exit;
            write.await
        });
        match landed.await {
            Ok(outcome) => outcome,
            Err(aborted) if aborted.is_panic() => std::panic::resume_unwind(aborted.into_panic()),
            Err(_) => Err(StorageError::Connection {
                reason: "catalog write aborted by runtime shutdown".to_owned(),
            }),
        }
    }
}

/// Advances when the detached write finishes, panics or is aborted, since the backend may have committed it.
struct AdvanceOnDrop(CatalogRevision);

impl Drop for AdvanceOnDrop {
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
        let inner = Arc::clone(&self.inner);
        let action = action.clone();
        self.revision
            .after(async move { inner.save(&action).await })
            .await
    }

    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.delete(id).await })
            .await
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
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.set_enabled(id, enabled).await })
            .await
    }

    async fn toggle_enabled(&self, id: ActionId) -> Result<Option<bool>, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.toggle_enabled(id).await })
            .await
    }

    async fn duplicate(
        &self,
        source_id: ActionId,
        new_id: ActionId,
        new_name: &str,
    ) -> Result<(), StorageError> {
        let inner = Arc::clone(&self.inner);
        let new_name = new_name.to_owned();
        self.revision
            .after(async move { inner.duplicate(source_id, new_id, &new_name).await })
            .await
    }

    async fn archive(&self, id: ActionId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.archive(id).await })
            .await
    }

    async fn restore(&self, id: ActionId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.restore(id).await })
            .await
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
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.link_action(action_id, instance_id, position).await })
            .await
    }

    async fn unlink_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.unlink_action(action_id, instance_id).await })
            .await
    }

    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
        let inner = Arc::clone(&self.inner);
        let instance = instance.clone();
        self.revision
            .after(async move { inner.save(&instance).await })
            .await
    }

    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.delete(id).await })
            .await
    }

    async fn upsert_default(
        &self,
        kind_id: &str,
        name: &str,
    ) -> Result<TriggerInstanceId, StorageError> {
        let inner = Arc::clone(&self.inner);
        let kind_id = kind_id.to_owned();
        let name = name.to_owned();
        self.revision
            .after(async move { inner.upsert_default(&kind_id, &name).await })
            .await
    }

    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.set_enabled(id, enabled).await })
            .await
    }

    async fn archive(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.archive(id).await })
            .await
    }

    async fn restore(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.restore(id).await })
            .await
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
        let inner = Arc::clone(&self.inner);
        let queue = queue.clone();
        self.revision
            .after(async move { inner.save(&queue).await })
            .await
    }

    async fn delete(&self, id: QueueId) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after(async move { inner.delete(id).await })
            .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::pin::pin;
    use std::task::Poll;

    use tokio::sync::Semaphore;

    use super::*;

    /// A queue repo whose `save` blocks until the test hands it a permit.
    struct GatedQueueRepo {
        permits: Arc<Semaphore>,
    }

    #[async_trait]
    impl QueueRepo for GatedQueueRepo {
        async fn list(&self) -> Result<Vec<Queue>, StorageError> {
            Ok(Vec::new())
        }
        async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
            Ok(None)
        }
        async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
            Ok(None)
        }
        async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
            self.permits.acquire().await.unwrap().forget();
            Ok(())
        }
        async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
            Ok(false)
        }
    }

    fn gated() -> (Arc<dyn QueueRepo>, Arc<Semaphore>, CatalogRevision) {
        let permits = Arc::new(Semaphore::new(0));
        let revision = CatalogRevision::new();
        let repo = RevisingQueueRepo::wrap(
            Arc::new(GatedQueueRepo {
                permits: Arc::clone(&permits),
            }),
            revision.clone(),
        );
        (repo, permits, revision)
    }

    fn queue() -> Queue {
        Queue {
            id: QueueId::new(),
            name: "q".to_owned(),
            description: String::new(),
            concurrency: 1,
        }
    }

    async fn poll_once<F: Future>(fut: std::pin::Pin<&mut F>) -> Poll<F::Output> {
        let mut fut = fut;
        std::future::poll_fn(|cx| Poll::Ready(fut.as_mut().poll(cx))).await
    }

    #[tokio::test]
    async fn revision_advances_only_once_the_write_returns() {
        let (repo, permits, revision) = gated();
        let queue = queue();
        let mut save = pin!(repo.save(&queue));

        assert!(poll_once(save.as_mut()).await.is_pending());
        let while_in_flight = revision.current();
        permits.add_permits(1);
        save.await.unwrap();

        assert_eq!(
            (while_in_flight, revision.current()),
            (0, 1),
            "an in-flight write must not be visible as a new revision until it returns",
        );
    }

    #[tokio::test]
    async fn cancelled_in_flight_write_still_advances_the_revision() {
        let (repo, _permits, revision) = gated();
        let queue = queue();
        {
            let mut save = pin!(repo.save(&queue));
            assert!(poll_once(save.as_mut()).await.is_pending());
        }

        assert_eq!(
            revision.current(),
            1,
            "a dropped write may still commit, so readers must be told to rebuild",
        );
    }
}
