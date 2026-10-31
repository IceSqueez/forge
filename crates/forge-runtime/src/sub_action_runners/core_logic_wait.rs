use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, DurationBounds, DurationField, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

const MAX_DELAY_MS: i64 = 60_000;
const DEFAULT_DELAY_MS: i64 = 1_000;

pub const WAIT_KIND_ID: &str = "core.logic.wait";
pub const WAIT_MS_KEY: &str = "ms";

const DELAY: DurationField = DurationField {
    key: WAIT_MS_KEY,
    label: "Duration",
    default: DEFAULT_DELAY_MS,
    bounds: DurationBounds::in_millis(0, MAX_DELAY_MS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&DELAY]);

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, WAIT_KIND_ID);
        let delay = match DELAY.read(config, ctx.arg_stack) {
            Ok(delay) => delay,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = ctx.cancel.cancelled() => {}
        }

        (timer.success(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_registry::CancelSignal;
    use forge_types::Variant;
    use forge_types::{EventId, SubActionOutcome};
    use std::time::Duration;

    const TIMER_GRANULARITY: Duration = Duration::from_millis(2);

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    async fn run_wait(configured_ms: i64) -> (SubActionOutcome, Duration) {
        let mut cfg = SubActionConfig::new();
        cfg.insert(WAIT_MS_KEY.to_owned(), Variant::Int(configured_ms));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let before = tokio::time::Instant::now();
        let (telemetry, updated) = CoreLogicWaitRunner.execute(&cfg, &ctx).await;
        assert!(updated.is_none(), "a wait produces no arguments");
        (telemetry.outcome, before.elapsed())
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_holds_for_its_own_delay_up_to_and_including_the_cap() {
        for held_ms in [0, 1_500, MAX_DELAY_MS] {
            let (outcome, held) = run_wait(held_ms).await;
            let expected = Duration::from_millis(held_ms.unsigned_abs());
            assert!(matches!(outcome, SubActionOutcome::Success), "{held_ms}");
            assert!(
                held >= expected && held < expected + TIMER_GRANULARITY,
                "{held_ms} ms held {held:?}",
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_outside_its_bounds_fails_without_holding() {
        for configured_ms in [-5, MAX_DELAY_MS + 1] {
            let (outcome, held) = run_wait(configured_ms).await;
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if crate::test_support::failure::rejected_field(m) == Some(WAIT_MS_KEY)),
                "{configured_ms}: {outcome:?}"
            );
            assert!(held < TIMER_GRANULARITY, "{configured_ms} held {held:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_run_leaves_a_long_wait_at_the_moment_of_the_cancel() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(WAIT_MS_KEY.to_owned(), Variant::Int(MAX_DELAY_MS));
        let stack = ArgStack::new();
        let mut ctx = make_ctx(&stack);
        let cancel = CancelSignal::new();
        ctx.cancel = cancel.clone();
        let cancel_after = Duration::from_secs(1);
        tokio::spawn(async move {
            tokio::time::sleep(cancel_after).await;
            cancel.cancel();
        });

        let before = tokio::time::Instant::now();
        CoreLogicWaitRunner.execute(&cfg, &ctx).await;
        let held = before.elapsed();

        assert!(
            held >= cancel_after && held < cancel_after + TIMER_GRANULARITY,
            "a wait cancelled at {cancel_after:?} held {held:?}",
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_run_cancelled_before_the_wait_does_not_hold_at_all() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(WAIT_MS_KEY.to_owned(), Variant::Int(MAX_DELAY_MS));
        let stack = ArgStack::new();
        let mut ctx = make_ctx(&stack);
        let cancel = CancelSignal::new();
        cancel.cancel();
        ctx.cancel = cancel;

        let before = tokio::time::Instant::now();
        CoreLogicWaitRunner.execute(&cfg, &ctx).await;

        assert!(
            before.elapsed() < TIMER_GRANULARITY,
            "a run cancelled up front held {:?}",
            before.elapsed(),
        );
    }
}
