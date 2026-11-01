use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_types::{ActionId, EventId, Variant};
use time::OffsetDateTime;

use crate::{CatalogRevision, StorageError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunRefusal {
    Missed,
    Superseded,
    Cancelled,
    ActionRemoved,
    ActionDisabled,
    ActionNotFound,
    ActionsUnavailable,
    QueuesClosed,
    Unreadable,
    QueuePaused,
    QueueDraining,
    QueueOverflow,
    QueueNotFound,
    QueueDeleted,
    EngineStopped,
}

impl RunRefusal {
    pub const ALL: [Self; 15] = [
        Self::Missed,
        Self::Superseded,
        Self::Cancelled,
        Self::ActionRemoved,
        Self::ActionDisabled,
        Self::ActionNotFound,
        Self::ActionsUnavailable,
        Self::QueuesClosed,
        Self::Unreadable,
        Self::QueuePaused,
        Self::QueueDraining,
        Self::QueueOverflow,
        Self::QueueNotFound,
        Self::QueueDeleted,
        Self::EngineStopped,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Missed => "missed",
            Self::Superseded => "superseded",
            Self::Cancelled => "cancelled",
            Self::ActionRemoved => "action_removed",
            Self::ActionDisabled => "action_disabled",
            Self::ActionNotFound => "action_not_found",
            Self::ActionsUnavailable => "actions_unavailable",
            Self::QueuesClosed => "queues_closed",
            Self::Unreadable => "unreadable",
            Self::QueuePaused => "queue_paused",
            Self::QueueDraining => "queue_draining",
            Self::QueueOverflow => "queue_pending_overflow",
            Self::QueueNotFound => "queue_not_found",
            Self::QueueDeleted => "queue_deleted",
            Self::EngineStopped => "engine_stopped",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.code() == code)
    }
}

impl fmt::Display for RunRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduledRunId(i64);

impl ScheduledRunId {
    pub fn new(raw: i64) -> Self {
        Self(raw)
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedRunPolicy {
    RunLateOnce,
    SkipIfLateBy(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledRunState {
    Pending,
    Dispatched,
    Cancelled,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledRunOutcome {
    Dispatched,
    Cancelled,
    Skipped,
    Failed,
}

impl From<ScheduledRunOutcome> for ScheduledRunState {
    fn from(outcome: ScheduledRunOutcome) -> Self {
        match outcome {
            ScheduledRunOutcome::Dispatched => Self::Dispatched,
            ScheduledRunOutcome::Cancelled => Self::Cancelled,
            ScheduledRunOutcome::Skipped => Self::Skipped,
            ScheduledRunOutcome::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledRunSpec {
    pub target_action_id: ActionId,
    pub due_at: OffsetDateTime,
    pub key: Option<String>,
    pub missed_run_policy: MissedRunPolicy,
    pub args: BTreeMap<String, Variant>,
    pub scheduled_by_action: Option<ActionId>,
    pub scheduled_by_run: Option<EventId>,
    pub trigger_event_id: Option<EventId>,
    pub replay: bool,
    pub scheduled_at: OffsetDateTime,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledRun {
    pub id: ScheduledRunId,
    pub spec: ScheduledRunSpec,
    pub state: ScheduledRunState,
    pub outcome_reason: Option<RunRefusal>,
    pub resolved_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledRunPlacement {
    pub id: ScheduledRunId,
    pub superseded: Option<ScheduledRunId>,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ScheduledRunRepo: Send + Sync {
    async fn schedule(
        &self,
        spec: &ScheduledRunSpec,
    ) -> Result<ScheduledRunPlacement, StorageError>;

    async fn get(&self, id: ScheduledRunId) -> Result<Option<ScheduledRun>, StorageError>;
    async fn list_pending(&self) -> Result<Vec<ScheduledRun>, StorageError>;

    async fn list_due(&self, now: OffsetDateTime) -> Result<Vec<ScheduledRun>, StorageError>;

    async fn fail_unreadable_due(
        &self,
        now: OffsetDateTime,
        reason: RunRefusal,
    ) -> Result<Vec<ScheduledRunId>, StorageError>;

    async fn next_due(&self) -> Result<Option<OffsetDateTime>, StorageError>;
    async fn claim(
        &self,
        id: ScheduledRunId,
        claimed_at: OffsetDateTime,
    ) -> Result<Option<ScheduledRun>, StorageError>;

    async fn cancel(&self, id: ScheduledRunId, at: OffsetDateTime) -> Result<bool, StorageError>;

    async fn cancel_by_key(&self, key: &str, at: OffsetDateTime) -> Result<bool, StorageError>;
    async fn settle(
        &self,
        id: ScheduledRunId,
        outcome: ScheduledRunOutcome,
        reason: Option<RunRefusal>,
        at: OffsetDateTime,
    ) -> Result<bool, StorageError>;

    async fn list_recent_resolved(&self, limit: usize) -> Result<Vec<ScheduledRun>, StorageError>;

    async fn prune_resolved_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;

    async fn count_pending(&self) -> Result<u64, StorageError>;

    async fn count_pending_for_action(&self, action_id: ActionId) -> Result<u64, StorageError>;
}

pub struct RevisingScheduledRunRepo {
    inner: Arc<dyn ScheduledRunRepo>,
    revision: CatalogRevision,
}

impl RevisingScheduledRunRepo {
    pub fn wrap(
        inner: Arc<dyn ScheduledRunRepo>,
        revision: CatalogRevision,
    ) -> Arc<dyn ScheduledRunRepo> {
        Arc::new(Self { inner, revision })
    }
}

#[async_trait]
impl ScheduledRunRepo for RevisingScheduledRunRepo {
    async fn schedule(
        &self,
        spec: &ScheduledRunSpec,
    ) -> Result<ScheduledRunPlacement, StorageError> {
        let inner = Arc::clone(&self.inner);
        let spec = spec.clone();
        self.revision
            .after(async move { inner.schedule(&spec).await })
            .await
    }

    async fn get(&self, id: ScheduledRunId) -> Result<Option<ScheduledRun>, StorageError> {
        self.inner.get(id).await
    }

    async fn list_pending(&self) -> Result<Vec<ScheduledRun>, StorageError> {
        self.inner.list_pending().await
    }

    async fn list_due(&self, now: OffsetDateTime) -> Result<Vec<ScheduledRun>, StorageError> {
        self.inner.list_due(now).await
    }

    async fn next_due(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        self.inner.next_due().await
    }

    async fn fail_unreadable_due(
        &self,
        now: OffsetDateTime,
        reason: RunRefusal,
    ) -> Result<Vec<ScheduledRunId>, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after_when(
                async move { inner.fail_unreadable_due(now, reason).await },
                |failed| !failed.is_empty(),
            )
            .await
    }

    async fn claim(
        &self,
        id: ScheduledRunId,
        claimed_at: OffsetDateTime,
    ) -> Result<Option<ScheduledRun>, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after_when(
                async move { inner.claim(id, claimed_at).await },
                Option::is_some,
            )
            .await
    }

    async fn cancel(&self, id: ScheduledRunId, at: OffsetDateTime) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after_when(async move { inner.cancel(id, at).await }, |changed| {
                *changed
            })
            .await
    }

    async fn cancel_by_key(&self, key: &str, at: OffsetDateTime) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_owned();
        self.revision
            .after_when(
                async move { inner.cancel_by_key(&key, at).await },
                |changed| *changed,
            )
            .await
    }

    async fn settle(
        &self,
        id: ScheduledRunId,
        outcome: ScheduledRunOutcome,
        reason: Option<RunRefusal>,
        at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after_when(
                async move { inner.settle(id, outcome, reason, at).await },
                |changed| *changed,
            )
            .await
    }

    async fn list_recent_resolved(&self, limit: usize) -> Result<Vec<ScheduledRun>, StorageError> {
        self.inner.list_recent_resolved(limit).await
    }

    async fn prune_resolved_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let inner = Arc::clone(&self.inner);
        self.revision
            .after_when(
                async move { inner.prune_resolved_before(cutoff).await },
                |deleted| *deleted > 0,
            )
            .await
    }

    async fn count_pending(&self) -> Result<u64, StorageError> {
        self.inner.count_pending().await
    }

    async fn count_pending_for_action(&self, action_id: ActionId) -> Result<u64, StorageError> {
        self.inner.count_pending_for_action(action_id).await
    }
}
