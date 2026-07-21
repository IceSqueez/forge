use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use super::core_queue_shared::{resolve_queue_id, validate_queue_id};
use crate::SchedulerCell;

pub struct CoreQueueResumeRunner {
    scheduler: SchedulerCell,
}

impl CoreQueueResumeRunner {
    pub fn new(scheduler: SchedulerCell) -> Self {
        Self { scheduler }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let Some(queue_id) = resolve_queue_id(config, ctx) else {
            return SubActionOutcome::Failed("core.queue.resume: invalid queue_id".to_owned());
        };
        let Some(scheduler) = self.scheduler.get() else {
            return SubActionOutcome::Failed("queue scheduler not ready".to_owned());
        };
        match scheduler.resume(queue_id).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("core.queue.resume: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreQueueResumeRunner {
    fn id(&self) -> &str {
        "core.queue.resume"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Resume Queue"
    }

    fn summary(&self) -> &str {
        "Let a paused queue start running actions again"
    }

    fn search_text(&self) -> &str {
        "resume queue unpause continue start"
    }

    fn icon_name(&self) -> &str {
        "play"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("queue_id".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "queue_id",
            label: "Queue",
            options_key: "queue.ids",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_queue_id(config, "core.queue.resume")
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.queue.resume");
        let outcome = self.run(config, ctx).await;
        (timer.finish(outcome), None)
    }
}
