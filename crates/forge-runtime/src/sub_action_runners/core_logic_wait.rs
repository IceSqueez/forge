use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    CancelSignal, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};

const MAX_DELAY_MS: u64 = 60_000;
const CANCEL_POLL_MS: u64 = 50;

pub const WAIT_KIND_ID: &str = "core.logic.wait";
pub const WAIT_MS_KEY: &str = "ms";

pub struct CoreLogicWaitRunner;

#[async_trait]
impl SubActionRunner for CoreLogicWaitRunner {
    fn id(&self) -> &str {
        WAIT_KIND_ID
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
        cfg.insert(WAIT_MS_KEY.to_owned(), Variant::Int(1000));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: WAIT_MS_KEY,
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
        let timer = StepTimer::start(ctx, WAIT_KIND_ID);

        let ms = config.int(WAIT_MS_KEY).unwrap_or(0).max(0) as u64;
        let delay = Duration::from_millis(ms.min(MAX_DELAY_MS));

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = wait_for_cancel(&ctx.cancel) => {}
        }

        (timer.success(), None)
    }
}

async fn wait_for_cancel(cancel: &CancelSignal) {
    let mut poll = tokio::time::interval(Duration::from_millis(CANCEL_POLL_MS));
    loop {
        poll.tick().await;
        if cancel.is_cancelled() {
            return;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};

    // Why: the timer wheel rounds a deadline up to its next tick, so the virtual clock
    // can land a hair past the requested delay.
    const TIMER_GRANULARITY: Duration = Duration::from_millis(2);

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_holds_for_its_own_delay_up_to_the_cap_and_never_past_it() {
        for (configured_ms, held_ms) in [
            (-5_i64, 0_u64),
            (0, 0),
            (1_500, 1_500),
            (MAX_DELAY_MS as i64, MAX_DELAY_MS),
            (MAX_DELAY_MS as i64 + 1, MAX_DELAY_MS),
            (120_000, MAX_DELAY_MS),
        ] {
            let runner = CoreLogicWaitRunner;
            let mut cfg = SubActionConfig::new();
            cfg.insert(WAIT_MS_KEY.to_owned(), Variant::Int(configured_ms));
            let stack = ArgStack::new();
            let ctx = make_ctx(&stack);

            let before = tokio::time::Instant::now();
            let (telemetry, updated) = runner.execute(&cfg, &ctx).await;
            let held = before.elapsed();

            assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
            assert!(updated.is_none(), "a wait produces no arguments");
            assert!(
                held >= Duration::from_millis(held_ms)
                    && held < Duration::from_millis(held_ms) + TIMER_GRANULARITY,
                "{configured_ms} ms held {held:?}, expected about {held_ms} ms",
            );
        }
    }
}
