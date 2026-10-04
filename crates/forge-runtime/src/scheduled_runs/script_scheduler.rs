use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_script::{
    ActionScheduler, ScriptScheduleDue, ScriptScheduleError, ScriptSchedulePlacement,
    ScriptScheduleRequest,
};
use forge_storage::{ActionRepo, MissedRunPolicy};
use forge_types::ActionId;

use super::cell::ScheduledRunsCell;
use super::handle::ScheduledRunsHandle;
use super::request::{ScheduleDue, ScheduleError, skip_if_late_by_minutes};
use super::scheduling_context::{ScheduleIntent, SchedulingContext};
use crate::sub_action_runners::datetime_input::resolve_datetime;

#[derive(Clone)]
pub struct ScriptScheduling {
    scheduled_runs: ScheduledRunsCell,
    actions: Arc<dyn ActionRepo>,
}

impl ScriptScheduling {
    pub fn new(scheduled_runs: ScheduledRunsCell, actions: Arc<dyn ActionRepo>) -> Self {
        Self {
            scheduled_runs,
            actions,
        }
    }

    pub fn for_context(&self, context: SchedulingContext) -> Arc<dyn ActionScheduler> {
        Arc::new(ContextScheduler {
            scheduling: self.clone(),
            context,
        })
    }

    fn handle(&self) -> Result<ScheduledRunsHandle, ScriptScheduleError> {
        self.scheduled_runs
            .get()
            .ok_or_else(|| rejected(ScheduleError::SchedulerStopped))
    }

    async fn resolve_action(&self, id_or_name: &str) -> Result<ActionId, ScriptScheduleError> {
        let wanted = id_or_name.trim();
        if let Ok(id) = wanted.parse::<ActionId>() {
            return Ok(id);
        }
        let actions = self
            .actions
            .list()
            .await
            .map_err(|e| ScriptScheduleError::ActionsUnavailable(e.to_string()))?;
        let mut named = actions.iter().filter(|action| action.name == wanted);
        match (named.next(), named.count()) {
            (Some(action), 0) => Ok(action.id),
            (Some(_), others) => Err(ScriptScheduleError::AmbiguousAction {
                name: wanted.to_owned(),
                count: others + 1,
            }),
            (None, _) => Err(ScriptScheduleError::UnknownAction(wanted.to_owned())),
        }
    }
}

struct ContextScheduler {
    scheduling: ScriptScheduling,
    context: SchedulingContext,
}

#[async_trait]
impl ActionScheduler for ContextScheduler {
    async fn schedule(
        &self,
        request: ScriptScheduleRequest,
    ) -> Result<ScriptSchedulePlacement, ScriptScheduleError> {
        let handle = self.scheduling.handle()?;
        let target_action_id = self
            .scheduling
            .resolve_action(&request.action_id_or_name)
            .await?;
        let intent = ScheduleIntent {
            target_action_id,
            due: schedule_due(request.due)?,
            key: request.key,
            missed_run_policy: request
                .skip_if_late_minutes
                .map_or(MissedRunPolicy::RunLateOnce, skip_if_late_by_minutes),
            inherit_args: request.inherit_args,
        };
        let placement = handle
            .schedule(self.context.request(intent))
            .await
            .map_err(rejected)?;
        Ok(ScriptSchedulePlacement {
            id: placement.id.get(),
            due_at: placement.due_at,
        })
    }

    async fn cancel_by_key(&self, key: &str) -> Result<bool, ScriptScheduleError> {
        self.scheduling
            .handle()?
            .cancel_by_key(key)
            .await
            .map_err(rejected)
    }
}

fn schedule_due(due: ScriptScheduleDue) -> Result<ScheduleDue, ScriptScheduleError> {
    match due {
        ScriptScheduleDue::AfterSeconds(seconds) => Ok(ScheduleDue::After(Duration::from_secs(
            u64::try_from(seconds).unwrap_or(0),
        ))),
        ScriptScheduleDue::At(when) => resolve_datetime(&when, when.as_str().unwrap_or(""))
            .map(ScheduleDue::At)
            .map_err(|e| ScriptScheduleError::Rejected(format!("due date-time: {e}"))),
    }
}

fn rejected(error: ScheduleError) -> ScriptScheduleError {
    ScriptScheduleError::Rejected(error.to_string())
}
