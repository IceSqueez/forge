use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::core_queue_shared::{resolve_queue_id, validate_queue_id};
use crate::SchedulerCell;

pub struct CoreQueueClearRunner {
    scheduler: SchedulerCell,
}

impl CoreQueueClearRunner {
    pub fn new(scheduler: SchedulerCell) -> Self {
        Self { scheduler }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let Some(queue_id) = resolve_queue_id(config, ctx) else {
            return SubActionOutcome::Failed("core.queue.clear: invalid queue_id".to_owned());
        };
        let keep_current = config
            .get("keep_current")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let Some(scheduler) = self.scheduler.get() else {
            return SubActionOutcome::Failed("queue scheduler not ready".to_owned());
        };
        match scheduler.clear(queue_id, keep_current).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("core.queue.clear: {e}")),
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("queue_id".to_owned(), Variant::String(String::new()));
        cfg.insert("keep_current".to_owned(), Variant::Bool(true));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "queue_id",
                label: "Queue",
                options_key: "queue.ids",
            },
            FormField::Toggle {
                key: "keep_current",
                label: "Keep running action",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_queue_id(config, "core.queue.clear")
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
                kind: "core.queue.clear".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
