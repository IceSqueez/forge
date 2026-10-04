use forge_storage::{ScheduledRun, ScheduledRunId};
use forge_types::{ActionId, EventId, ExecutionMetadata};

#[derive(Debug, Clone)]
pub(crate) struct ScheduledOrigin {
    scheduled_run_id: ScheduledRunId,
    scheduled_by_action: Option<ActionId>,
    scheduled_by_run: Option<String>,
}

impl ScheduledOrigin {
    pub(super) fn of(run: &ScheduledRun) -> Self {
        Self {
            scheduled_run_id: run.id,
            scheduled_by_action: run.spec.scheduled_by_action,
            scheduled_by_run: run.spec.scheduled_by_run.clone(),
        }
    }

    pub(crate) fn into_metadata(self, event_id: EventId) -> ExecutionMetadata {
        ExecutionMetadata::Scheduled {
            event_id,
            scheduled_run_id: self.scheduled_run_id.get(),
            scheduled_by_action: self.scheduled_by_action,
            scheduled_by_run: self.scheduled_by_run,
        }
    }
}
