use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};

const MAX_DELAY_MS: u64 = 60_000;

pub struct CoreLogicWaitRunner;

#[async_trait]
impl SubActionRunner for CoreLogicWaitRunner {
    fn id(&self) -> &str {
        "core.logic.wait"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Delay
    }

    fn label(&self) -> &str {
        "Wait"
    }

    fn summary(&self) -> &str {
        "Pause execution for a number of milliseconds (capped at 60s)"
    }

    fn search_text(&self) -> &str {
        "wait delay sleep pause ms milliseconds"
    }

    fn icon_name(&self) -> &str {
        "clock-pause"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("ms".to_owned(), Variant::Int(1000));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "ms",
            label: "Milliseconds",
            min: 0,
            max: MAX_DELAY_MS as i64,
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.logic.wait");

        let ms = config.int("ms").unwrap_or(0).max(0) as u64;

        tokio::time::sleep(Duration::from_millis(ms.min(MAX_DELAY_MS))).await;

        (timer.success(), None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    #[tokio::test]
    async fn wait_zero_ms_succeeds() {
        let runner = CoreLogicWaitRunner;
        let mut cfg = SubActionConfig::new();
        cfg.insert("ms".to_owned(), Variant::Int(0));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, updated) = runner.execute(&cfg, &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn wait_above_cap_is_clamped() {
        let runner = CoreLogicWaitRunner;
        let mut cfg = SubActionConfig::new();
        cfg.insert("ms".to_owned(), Variant::Int(120_000));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let before = std::time::Instant::now();
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(
            before.elapsed().as_millis() < 65_000,
            "clamped delay must not exceed 60s + tolerance"
        );
    }
}
