use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, QueueId, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_queue_shared::{QUEUE, QUEUE_SPEC};
use crate::core_messages;
use crate::{QueueMode, QueueSchedulerHandle};

pub struct CoreQueuePauseRunner {
    scheduler: QueueSchedulerHandle,
}

impl CoreQueuePauseRunner {
    pub fn new(scheduler: QueueSchedulerHandle) -> Self {
        Self { scheduler }
    }

    async fn run(&self, queue_id: QueueId) -> SubActionOutcome {
        match self.scheduler.set_mode(queue_id, QueueMode::PAUSED).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(core_messages::scheduler_failed(e).into()),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreQueuePauseRunner {
    fn id(&self) -> &str {
        "core.queue.pause"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Pause Queue"
    }

    fn summary(&self) -> &str {
        "Stop a queue from starting new actions until resumed"
    }

    fn search_text(&self) -> &str {
        "pause queue hold stop suspend"
    }

    fn icon_name(&self) -> &str {
        "pause"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(QUEUE_SPEC)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let queue_id = match QUEUE.read(config, ctx.arg_stack) {
            Ok(queue_id) => queue_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(queue_id).await;
        (timer.finish(outcome), None)
    }
}
