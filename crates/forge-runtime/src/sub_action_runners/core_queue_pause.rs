use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::core_queue_shared::{resolve_queue_id, validate_queue_id};
use crate::SchedulerCell;

pub struct CoreQueuePauseRunner {
    scheduler: SchedulerCell,
}

impl CoreQueuePauseRunner {
    pub fn new(scheduler: SchedulerCell) -> Self {
        Self { scheduler }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let Some(queue_id) = resolve_queue_id(config, ctx) else {
            return SubActionOutcome::Failed("core.queue.pause: invalid queue_id".to_owned());
        };
        let Some(scheduler) = self.scheduler.get() else {
            return SubActionOutcome::Failed("queue scheduler not ready".to_owned());
        };
        match scheduler.pause(queue_id).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("core.queue.pause: {e}")),
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
        validate_queue_id(config, "core.queue.pause")
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let outcome = self.run(config, ctx).await;
        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.queue.pause".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
