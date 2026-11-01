use std::sync::Arc;

use forge_storage::{QueueRepo, StorageError};
use forge_types::{Queue, QueueId};

use crate::engine::queue_scheduler::{MembershipOutcome, QueueSchedulerHandle};
use crate::error::RuntimeError;

#[derive(Debug, thiserror::Error)]
pub enum QueueApplyError {
    #[error("the queue no longer exists")]
    Missing,
    #[error("the scheduler lost the queue")]
    SchedulerLost,
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

#[derive(Clone)]
pub struct QueueService {
    repo: Arc<dyn QueueRepo>,
    scheduler: QueueSchedulerHandle,
}

impl QueueService {
    pub fn new(repo: Arc<dyn QueueRepo>, scheduler: QueueSchedulerHandle) -> Self {
        Self { repo, scheduler }
    }

    pub async fn save_live(
        &self,
        queue: Queue,
    ) -> Result<Result<MembershipOutcome, RuntimeError>, StorageError> {
        self.repo.save(&queue).await?;
        Ok(self.scheduler.ensure_live(queue).await)
    }

    pub async fn apply_live(&self, id: QueueId) -> Result<(), QueueApplyError> {
        let queue = self
            .repo
            .list()
            .await?
            .into_iter()
            .find(|queue| queue.id == id)
            .ok_or(QueueApplyError::Missing)?;
        match self.scheduler.ensure_live(queue).await? {
            MembershipOutcome::Applied | MembershipOutcome::AlreadyRegistered => Ok(()),
            MembershipOutcome::NotFound => Err(QueueApplyError::SchedulerLost),
        }
    }

    pub async fn delete(&self, id: QueueId) -> Result<(), StorageError> {
        let deleted = self.repo.delete_reassigning_actions(id).await?;
        if deleted.is_some()
            && let Err(error) = self.scheduler.deregister(id).await
        {
            tracing::warn!(%error, "queue deregister failed");
        }
        Ok(())
    }
}
