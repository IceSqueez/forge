use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    CancelSignal, CodeField, CodeKind, CodeLanguage, ConfigProblem, ConfigSpec, DurationBounds,
    DurationField, RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner,
    TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};
use tokio::time::Instant;

use crate::ConditionGate;

const POLL_MIN_MS: i64 = 100;
const POLL_MAX_MS: i64 = 30_000;
const DEFAULT_POLL_MS: i64 = 500;
const TIMEOUT_MIN_MS: i64 = 100;
const TIMEOUT_MAX_MS: i64 = 600_000;
const DEFAULT_TIMEOUT_MS: i64 = 30_000;
const CANCEL_POLL_MS: u64 = 50;

const WAIT_ELAPSED_MS_VAR: &str = "wait.elapsed_ms";
const WAIT_TIMED_OUT_VAR: &str = "wait.timed_out";

const CONDITION: CodeField = CodeField {
    key: "condition",
    label: "Condition",
    language: CodeLanguage::Rhai,
    kind: CodeKind::Expression,
    default: "",
    required: true,
};

const POLL_INTERVAL: DurationField = DurationField {
    key: "poll_interval_ms",
    label: "Poll Interval",
    default: DEFAULT_POLL_MS,
    bounds: DurationBounds::in_millis(POLL_MIN_MS, POLL_MAX_MS),
};

const TIMEOUT: DurationField = DurationField {
    key: "timeout_ms",
    label: "Timeout",
    default: DEFAULT_TIMEOUT_MS,
    bounds: DurationBounds::in_millis(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CONDITION, &POLL_INTERVAL, &TIMEOUT]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, Duration, Duration), ConfigProblem> {
    Ok((
        CONDITION.read(config, args)?,
        POLL_INTERVAL.read(config, args)?,
        TIMEOUT.read(config, args)?,
    ))
}

pub struct CoreLogicWaitUntilRunner {
    gate: Arc<ConditionGate>,
}

impl CoreLogicWaitUntilRunner {
    pub fn new(gate: Arc<ConditionGate>) -> Self {
        Self { gate }
    }
}

#[async_trait]
impl SubActionRunner for CoreLogicWaitUntilRunner {
    fn id(&self) -> &str {
        "core.logic.wait_until"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Delay
    }

    fn label(&self) -> &str {
        "Wait Until"
    }

    fn summary(&self) -> &str {
        "Poll a condition until it holds or a timeout elapses"
    }

    fn search_text(&self) -> &str {
        "wait until condition poll block timeout flow control"
    }

    fn icon_name(&self) -> &str {
        "hourglass"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                WAIT_ELAPSED_MS_VAR.to_owned(),
                WAIT_TIMED_OUT_VAR.to_owned(),
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (condition, poll_interval, timeout) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let begin = Instant::now();
        let deadline = begin + timeout;

        let mut timed_out = false;
        loop {
            if ctx.cancel.is_cancelled() {
                break;
            }

            if let Ok(true) = self
                .gate
                .evaluate_with_args(&condition, ctx.arg_stack)
                .await
            {
                break;
            }

            let now = Instant::now();
            if now >= deadline {
                timed_out = true;
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(poll_interval.min(deadline - now)) => {}
                _ = wait_for_cancel(&ctx.cancel) => {}
            }
        }

        let elapsed_ms = i64::try_from(begin.elapsed().as_millis()).unwrap_or(i64::MAX);
        let stack = ctx
            .arg_stack
            .clone()
            .set(WAIT_ELAPSED_MS_VAR.to_owned(), Variant::Int(elapsed_ms))
            .set(WAIT_TIMED_OUT_VAR.to_owned(), Variant::Bool(timed_out));

        (timer.success(), Some(stack))
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
mod tests {
    use super::*;
    use crate::Config;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    const TIMER_GRANULARITY: Duration = Duration::from_millis(2);

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[tokio::test(start_paused = true)]
    async fn a_run_cancelled_mid_poll_sleep_ends_within_one_cancel_poll() {
        let runner =
            CoreLogicWaitUntilRunner::new(Arc::new(ConditionGate::new(&Config::default())));
        let mut cfg = SubActionConfig::new();
        cfg.insert("condition".to_owned(), Variant::String("1 == 2".to_owned()));
        cfg.insert("poll_interval_ms".to_owned(), Variant::Int(POLL_MAX_MS));
        cfg.insert("timeout_ms".to_owned(), Variant::Int(TIMEOUT_MAX_MS));
        let stack = ArgStack::new();
        let mut ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let cancel = CancelSignal::new();
        ctx.cancel = cancel.clone();
        let cancel_after = Duration::from_secs(1);
        tokio::spawn(async move {
            tokio::time::sleep(cancel_after).await;
            cancel.cancel();
        });

        let before = Instant::now();
        let (_, out) = runner.execute(&cfg, &ctx).await;
        let held = before.elapsed();

        assert!(
            held >= cancel_after
                && held <= cancel_after + Duration::from_millis(CANCEL_POLL_MS) + TIMER_GRANULARITY,
            "a wait_until cancelled at {cancel_after:?} held {held:?}",
        );
        assert_eq!(
            out.unwrap().get("wait.timed_out"),
            Some(&Variant::Bool(false)),
            "a cancelled wait_until is not a timeout",
        );
    }
}
