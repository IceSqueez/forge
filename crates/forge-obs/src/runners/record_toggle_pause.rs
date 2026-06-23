use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct RecordTogglePauseRunner {
    sink: Arc<dyn ObsSink>,
}

impl RecordTogglePauseRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for RecordTogglePauseRunner {
    fn id(&self) -> &str {
        "obs.record.toggle_pause"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Toggle Recording Pause"
    }

    fn summary(&self) -> &str {
        "Toggles the pause state of the OBS recording output."
    }

    fn search_text(&self) -> &str {
        "obs record toggle pause resume unpause"
    }

    fn icon_name(&self) -> &str {
        "player-pause"
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

        let outcome = match self.sink.toggle_record_pause().await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "obs.record.toggle_pause".to_owned(),
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
        let runner = RecordTogglePauseRunner::new(Arc::new(MockSink));
        let (tel, extra) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.record.toggle_pause");
        assert!(extra.is_none());
    }
}
