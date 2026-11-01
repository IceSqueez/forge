use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, QueueId, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_queue_shared::{QUEUE, QUEUE_SPEC};
use crate::core_messages;
use crate::{QueueMode, QueueSchedulerHandle};

#[derive(Clone, Copy)]
struct StepCopy {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon_name: &'static str,
}

const PAUSE_COPY: StepCopy = StepCopy {
    id: "core.queue.pause",
    label: "Pause Queue",
    summary: "Stop a queue from starting new actions until resumed",
    search_text: "pause queue hold stop suspend",
    icon_name: "pause",
};

const RESUME_COPY: StepCopy = StepCopy {
    id: "core.queue.resume",
    label: "Resume Queue",
    summary: "Let a paused queue start running actions again",
    search_text: "resume queue unpause continue start",
    icon_name: "play",
};

pub(crate) struct CoreQueueModeRunner {
    scheduler: QueueSchedulerHandle,
    mode: QueueMode,
    copy: StepCopy,
}

impl CoreQueueModeRunner {
    pub(crate) fn pause(scheduler: QueueSchedulerHandle) -> Self {
        Self {
            scheduler,
            mode: QueueMode::PAUSED,
            copy: PAUSE_COPY,
        }
    }

    pub(crate) fn resume(scheduler: QueueSchedulerHandle) -> Self {
        Self {
            scheduler,
            mode: QueueMode::RUNNING,
            copy: RESUME_COPY,
        }
    }

    async fn run(&self, queue_id: QueueId) -> SubActionOutcome {
        match self.scheduler.set_mode(queue_id, self.mode).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(core_messages::scheduler_failed(e).into()),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreQueueModeRunner {
    fn id(&self) -> &str {
        self.copy.id
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        self.copy.label
    }

    fn summary(&self) -> &str {
        self.copy.summary
    }

    fn search_text(&self) -> &str {
        self.copy.search_text
    }

    fn icon_name(&self) -> &str {
        self.copy.icon_name
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
