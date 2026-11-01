use super::{QueueListing, QueueRow, SERIAL_CONCURRENCY};
use crate::shared::repo_failure::RepoFailure;
use forge_components::tr;
use forge_runtime::{QueueApplyError, QueueMode, QueueSchedulerHandle};
use forge_storage::{ActionRepo, QueueRepo};
use std::sync::Arc;

pub(super) async fn load_queues(
    queue_repo: Arc<dyn QueueRepo>,
    action_repo: Arc<dyn ActionRepo>,
    scheduler: QueueSchedulerHandle,
) -> Result<QueueListing, RepoFailure> {
    let queues = queue_repo.list().await.map_err(RepoFailure::from)?;
    let actions = action_repo.list().await.map_err(RepoFailure::from)?;
    let live_states = scheduler
        .queue_states()
        .await
        .inspect_err(|error| tracing::warn!(%error, "queue states could not be read"))
        .ok();
    let states = live_states.clone().unwrap_or_default();

    let rows = queues
        .into_iter()
        .map(|q| {
            let assigned = u32::try_from(actions.iter().filter(|a| a.queue_id == q.id).count())
                .unwrap_or(u32::MAX);
            let concurrency = q.concurrency.get();
            let mode = states.get(&q.id).map_or(QueueMode::RUNNING, |s| s.mode);
            let live = live_states.is_none() || states.contains_key(&q.id);
            QueueRow {
                id: q.id,
                name: q.name,
                description: q.description,
                blocking: concurrency == SERIAL_CONCURRENCY,
                concurrency,
                mode,
                live,
                pending: 0,
                in_flight: 0,
                overflowed: 0,
                actions: assigned,
                running: vec![],
            }
        })
        .collect();
    let action_names = actions
        .into_iter()
        .map(|action| (action.id, action.name))
        .collect();

    Ok(QueueListing { rows, action_names })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LiveApplyFailure {
    QueueMissing,
    SchedulerLostQueue,
    Failed(RepoFailure),
}

impl From<QueueApplyError> for LiveApplyFailure {
    fn from(error: QueueApplyError) -> Self {
        match error {
            QueueApplyError::Missing => Self::QueueMissing,
            QueueApplyError::SchedulerLost => Self::SchedulerLostQueue,
            QueueApplyError::Storage(error) => Self::Failed(error.into()),
            QueueApplyError::Runtime(error) => Self::Failed(error.into()),
        }
    }
}

impl LiveApplyFailure {
    pub(super) fn message(self) -> String {
        match self {
            Self::QueueMissing => tr!("queues_apply_live_missing"),
            Self::SchedulerLostQueue => tr!("queues_apply_live_not_found"),
            Self::Failed(failure) => failure.text(),
        }
    }
}
