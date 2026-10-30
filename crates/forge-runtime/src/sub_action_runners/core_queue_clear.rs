use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    ToggleField, TypedField,
};
use forge_types::{
    ArgStack, QueueId, SubActionConfig, SubActionOutcome, SubActionTelemetry, UserText,
};

use super::core_queue_shared::QUEUE;
use crate::SchedulerCell;

const KEEP_CURRENT: ToggleField = ToggleField {
    key: "keep_current",
    label: "Keep running action",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&QUEUE, &KEEP_CURRENT]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(QueueId, bool), ConfigProblem> {
    Ok((QUEUE.read(config, args)?, KEEP_CURRENT.read(config, args)?))
}

pub struct CoreQueueClearRunner {
    scheduler: SchedulerCell,
}

impl CoreQueueClearRunner {
    pub fn new(scheduler: SchedulerCell) -> Self {
        Self { scheduler }
    }

    async fn run(&self, queue_id: QueueId, keep_current: bool) -> SubActionOutcome {
        let Some(scheduler) = self.scheduler.get() else {
            return SubActionOutcome::Failed(UserText::unlocalized("queue scheduler not ready"));
        };
        match scheduler.clear(queue_id, keep_current).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => {
                SubActionOutcome::Failed(UserText::unlocalized(format!("core.queue.clear: {e}")))
            }
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreQueueClearRunner {
    fn id(&self) -> &str {
        "core.queue.clear"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Clear Queue"
    }

    fn summary(&self) -> &str {
        "Drop a queue's pending actions, optionally stopping the running one"
    }

    fn search_text(&self) -> &str {
        "clear queue empty flush drop pending purge"
    }

    fn icon_name(&self) -> &str {
        "trash"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (queue_id, keep_current) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(queue_id, keep_current).await;
        (timer.finish(outcome), None)
    }
}
