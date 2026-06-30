use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;
use tokio::time::Instant;

use super::core_logic_shared::telemetry;
use crate::ConditionGate;

const POLL_MIN_MS: i64 = 100;
const POLL_MAX_MS: i64 = 30_000;
const TIMEOUT_MIN_MS: i64 = 100;
const TIMEOUT_MAX_MS: i64 = 600_000;

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("condition".to_owned(), Variant::String(String::new()));
        cfg.insert("poll_interval_ms".to_owned(), Variant::Int(500));
        cfg.insert("timeout_ms".to_owned(), Variant::Int(30_000));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "condition",
                label: "Condition",
                placeholder: "%global.ready% == true",
            },
            FormField::Integer {
                key: "poll_interval_ms",
                label: "Poll Interval (ms)",
                min: POLL_MIN_MS,
                max: POLL_MAX_MS,
            },
            FormField::Integer {
                key: "timeout_ms",
                label: "Timeout (ms)",
                min: TIMEOUT_MIN_MS,
                max: TIMEOUT_MAX_MS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("condition").and_then(Variant::as_str) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.logic.wait_until: condition is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let begin = Instant::now();

        let template = config
            .get("condition")
            .and_then(Variant::as_str)
            .unwrap_or_default()
            .to_owned();
        let poll_interval = Duration::from_millis(
            config
                .get("poll_interval_ms")
                .and_then(Variant::as_int)
                .unwrap_or(500)
                .clamp(POLL_MIN_MS, POLL_MAX_MS) as u64,
        );
        let timeout = Duration::from_millis(
            config
                .get("timeout_ms")
                .and_then(Variant::as_int)
                .unwrap_or(30_000)
                .clamp(TIMEOUT_MIN_MS, TIMEOUT_MAX_MS) as u64,
        );
        let deadline = begin + timeout;

        let mut timed_out = false;
        loop {
            if ctx.cancel.is_cancelled() {
                break;
            }

            let expr = ctx.arg_stack.interpolate(&template);
            if let Ok(true) = self.gate.evaluate(&expr).await {
                break;
            }

            let now = Instant::now();
            if now >= deadline {
                timed_out = true;
                break;
            }
            tokio::time::sleep(poll_interval.min(deadline - now)).await;
        }

        let elapsed_ms = begin.elapsed().as_millis().min(i64::MAX as u128) as i64;
        let stack = ctx
            .arg_stack
            .clone()
            .set("wait.elapsed_ms".to_owned(), Variant::Int(elapsed_ms))
            .set("wait.timed_out".to_owned(), Variant::Bool(timed_out));

        (
            telemetry(ctx, self.id(), started_at, SubActionOutcome::Success),
            Some(stack),
        )
    }
}
