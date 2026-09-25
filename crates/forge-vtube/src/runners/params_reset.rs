use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::runners::hold_registry::HoldRegistry;
use crate::sink::VTubeSink;

pub struct ParamsResetRunner {
    sink: Arc<dyn VTubeSink>,
    holds: Arc<HoldRegistry>,
}

impl ParamsResetRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>, holds: Arc<HoldRegistry>) -> Self {
        Self { sink, holds }
    }
}

#[async_trait]
impl SubActionRunner for ParamsResetRunner {
    fn id(&self) -> &str {
        "vtube.params.reset"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Reset to Idle"
    }

    fn summary(&self) -> &str {
        "Deactivates every currently active expression, returning the model to idle."
    }

    fn search_text(&self) -> &str {
        "vtube expression reset idle deactivate default vts"
    }

    fn icon_name(&self) -> &str {
        "refresh"
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

        self.holds.abort_all();
        let outcome = SubActionOutcome::from_result(&self.sink.reset_params().await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.params.reset".to_owned(),
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

    #[tokio::test]
    async fn execute_reports_success_when_the_reset_succeeds() {
        let runner =
            ParamsResetRunner::new(Arc::new(MockSink::new()), Arc::new(HoldRegistry::new()));
        let stack = ArgStack::new();
        let (tel, extra) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
    }

    #[tokio::test]
    async fn execute_reports_failure_when_the_reset_fails() {
        let runner =
            ParamsResetRunner::new(Arc::new(MockSink::failing()), Arc::new(HoldRegistry::new()));
        let stack = ArgStack::new();
        let (tel, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;
        assert!(
            matches!(tel.outcome, SubActionOutcome::Failed(_)),
            "got {:?}",
            tel.outcome
        );
    }
}
