use std::collections::BTreeMap;

use forge_registry::{RunContext, RunningAction};
use forge_storage::MissedRunPolicy;
use forge_types::{ActionId, ArgStack, EventId};

use super::request::{ScheduleDue, ScheduleRequest};

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleIntent {
    pub target_action_id: ActionId,
    pub due: ScheduleDue,
    pub key: Option<String>,
    pub missed_run_policy: MissedRunPolicy,
    pub inherit_args: bool,
}

#[derive(Clone)]
pub struct SchedulingContext {
    args: ArgStack,
    scheduled_by: Option<RunningAction>,
    cause: Option<EventId>,
}

impl SchedulingContext {
    pub fn of_run(ctx: &RunContext<'_>) -> Self {
        Self {
            args: ctx.arg_stack.clone(),
            scheduled_by: ctx.executor.running_action(),
            cause: Some(ctx.parent_event_id),
        }
    }

    pub fn outside_any_run(args: ArgStack) -> Self {
        Self {
            args,
            scheduled_by: None,
            cause: None,
        }
    }

    pub fn request(&self, intent: ScheduleIntent) -> ScheduleRequest {
        ScheduleRequest {
            target_action_id: intent.target_action_id,
            due: intent.due,
            key: intent.key,
            missed_run_policy: intent.missed_run_policy,
            args: if intent.inherit_args {
                self.args.snapshot()
            } else {
                BTreeMap::new()
            },
            scheduled_by_action: self.scheduled_by.map(|run| run.action_id),
            scheduled_by_run: self.scheduled_by.map(|run| run.start_event_id.to_string()),
            trigger_event_id: self
                .scheduled_by
                .map(|run| run.trigger_event_id)
                .or(self.cause),
        }
    }
}
