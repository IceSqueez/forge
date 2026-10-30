use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};
use time::OffsetDateTime;

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::REPLAY_STOP;

pub struct ReplayStopRunner {
    sink: Arc<dyn ObsSink>,
}

impl ReplayStopRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ReplayStopRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Stop Replay Buffer"
    }

    fn summary(&self) -> &str {
        "Stops the OBS replay buffer output."
    }

    fn search_text(&self) -> &str {
        "obs replay buffer stop end halt"
    }

    fn icon_name(&self) -> &str {
        "player-stop"
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = step_failure::outcome(&self.sink.stop_replay_buffer().await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let runner = ReplayStopRunner::new(Arc::new(MockSink));
        let (tel, extra) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.replay.stop");
        assert!(extra.is_none());
    }
}
