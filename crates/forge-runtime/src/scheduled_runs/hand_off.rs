use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_storage::{
    ActionRepo, MissedRunPolicy, ScheduledRun, ScheduledRunId, ScheduledRunOutcome,
    ScheduledRunRepo, StorageError,
};
use forge_types::{Action, ActionId, EventId};
use serde_json::json;
use tracing::{info, warn};

use super::clock::WallClock;
use super::origin::ScheduledOrigin;
use super::provenance::{initial_args, lateness};
use crate::catalog::Catalog;
use crate::queue_scheduler::QUEUE_NOT_FOUND_REASON;
use crate::{EventBus, QueueSchedulerHandle, SchedulerError, SchedulerRequest};

pub const SCHEDULED_RUN_DUE_KIND: &str = "scheduled_run.due";

pub const MISSED_REASON: &str = "missed";
pub const ACTION_DISABLED_REASON: &str = "action_disabled";
pub const ACTION_NOT_FOUND_REASON: &str = "action_not_found";
pub const ACTIONS_UNAVAILABLE_REASON: &str = "actions_unavailable";
pub const QUEUES_CLOSED_REASON: &str = "queues_closed";
pub const UNREADABLE_REASON: &str = "unreadable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandOff {
    Dispatched,
    Skipped(&'static str),
    Failed(&'static str),
    NotPending,
}

enum Target {
    Ready(Arc<Action>),
    Refused(HandOff),
}

pub(super) struct HandOffPath {
    pub(super) repo: Arc<dyn ScheduledRunRepo>,
    pub(super) catalog: Arc<Catalog>,
    pub(super) actions: Arc<dyn ActionRepo>,
    pub(super) queues: QueueSchedulerHandle,
    pub(super) bus: Arc<EventBus>,
    pub(super) clock: Arc<dyn WallClock>,
}

impl HandOffPath {
    pub(super) async fn due(&self, run: ScheduledRun) -> Result<HandOff, StorageError> {
        let now = self.clock.now();
        let late = lateness(run.spec.due_at, now);
        if let MissedRunPolicy::SkipIfLateBy(tolerance) = run.spec.missed_run_policy
            && late > tolerance
        {
            let skipped = self
                .repo
                .settle(
                    run.id,
                    ScheduledRunOutcome::Skipped,
                    Some(MISSED_REASON.to_owned()),
                    now,
                )
                .await?;
            if !skipped {
                return Ok(HandOff::NotPending);
            }
            info!(
                scheduled_run = run.id.get(),
                late_secs = late.as_secs(),
                "scheduled run skipped: missed by more than its late tolerance"
            );
            return Ok(HandOff::Skipped(MISSED_REASON));
        }
        self.claim_and_hand_off(run.id, late).await
    }

    pub(super) async fn now(&self, id: ScheduledRunId) -> Result<HandOff, StorageError> {
        self.claim_and_hand_off(id, Duration::ZERO).await
    }

    async fn claim_and_hand_off(
        &self,
        id: ScheduledRunId,
        late: Duration,
    ) -> Result<HandOff, StorageError> {
        let Some(run) = self.repo.claim(id, self.clock.now()).await? else {
            return Ok(HandOff::NotPending);
        };
        let outcome = self.hand_off(&run, late).await;
        match outcome {
            HandOff::Dispatched | HandOff::NotPending => {}
            HandOff::Skipped(reason) => {
                self.settle(&run, ScheduledRunOutcome::Skipped, reason)
                    .await
            }
            HandOff::Failed(reason) => self.settle(&run, ScheduledRunOutcome::Failed, reason).await,
        }
        Ok(outcome)
    }

    async fn hand_off(&self, run: &ScheduledRun, late: Duration) -> HandOff {
        let action = match self.resolve(run.spec.target_action_id).await {
            Target::Ready(action) => action,
            Target::Refused(outcome) => return outcome,
        };
        let trigger_event_id = self.record_due(run, late);
        let request = SchedulerRequest {
            queue_id: action.queue_id,
            action_id: action.id,
            trigger_event_id,
            trigger_kind: None,
            initial_args: initial_args(run, late),
            bypass_pause: action.bypass_pause,
        };
        match self.queues.admit(request, ScheduledOrigin::of(run)).await {
            Ok(()) => {
                info!(
                    scheduled_run = run.id.get(),
                    action = %action.id,
                    late_secs = late.as_secs(),
                    "scheduled run handed off to its queue"
                );
                HandOff::Dispatched
            }
            Err(refusal) => HandOff::Failed(refusal_reason(&refusal)),
        }
    }

    async fn resolve(&self, action_id: ActionId) -> Target {
        match self.catalog.current().await {
            Ok(snapshot) => {
                if let Some(action) = snapshot.action(action_id) {
                    return Target::Ready(Arc::clone(action));
                }
            }
            Err(e) => {
                warn!(error = %e, "scheduled run: action catalog unavailable");
                return Target::Refused(HandOff::Failed(ACTIONS_UNAVAILABLE_REASON));
            }
        }
        match self.actions.get(action_id).await {
            Ok(Some(_)) => Target::Refused(HandOff::Skipped(ACTION_DISABLED_REASON)),
            Ok(None) => Target::Refused(HandOff::Failed(ACTION_NOT_FOUND_REASON)),
            Err(e) => {
                warn!(error = %e, "scheduled run: action lookup failed");
                Target::Refused(HandOff::Failed(ACTIONS_UNAVAILABLE_REASON))
            }
        }
    }

    fn record_due(&self, run: &ScheduledRun, late: Duration) -> EventId {
        let payload = json!({
            "scheduled_run_id": run.id.get(),
            "action_id": run.spec.target_action_id.to_string(),
            "late_seconds": late.as_secs(),
        });
        let event = match run.spec.trigger_event_id {
            Some(cause) => {
                Event::caused_by(EventSource::Core, SCHEDULED_RUN_DUE_KIND, payload, cause)
            }
            None => Event::new(EventSource::Core, SCHEDULED_RUN_DUE_KIND, payload),
        };
        let id = event.id;
        self.bus.record(event);
        id
    }

    async fn settle(&self, run: &ScheduledRun, outcome: ScheduledRunOutcome, reason: &str) {
        warn!(
            scheduled_run = run.id.get(),
            action = %run.spec.target_action_id,
            reason,
            "scheduled run not handed off"
        );
        let settled = self
            .repo
            .settle(run.id, outcome, Some(reason.to_owned()), self.clock.now())
            .await;
        if let Err(e) = settled {
            warn!(scheduled_run = run.id.get(), error = %e, "could not record the scheduled run outcome");
        }
    }
}

fn refusal_reason(refusal: &SchedulerError) -> &'static str {
    match refusal {
        SchedulerError::Refused(reason) => reason,
        SchedulerError::QueueNotFound(_) => QUEUE_NOT_FOUND_REASON,
        SchedulerError::ChannelClosed => QUEUES_CLOSED_REASON,
    }
}
