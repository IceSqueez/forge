use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct ReplayStartRunner {
    sink: Arc<dyn ObsSink>,
}

impl ReplayStartRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ReplayStartRunner {
    fn id(&self) -> &str {
        "obs.replay.start"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Start Replay Buffer"
    }

    fn summary(&self) -> &str {
        "Starts the OBS replay buffer output."
    }

    fn search_text(&self) -> &str {
        "obs replay buffer start begin"
    }

    fn icon_name(&self) -> &str {
        "player-record"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = match self.sink.start_replay_buffer().await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.replay.start".to_owned(),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let stack = ArgStack::new();
        let runner = ReplayStartRunner::new(Arc::new(MockSink));
        let (tel, extra) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.replay.start");
        assert!(extra.is_none());
    }
}
