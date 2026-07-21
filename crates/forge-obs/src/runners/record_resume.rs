use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::ObsSink;

pub struct RecordResumeRunner {
    sink: Arc<dyn ObsSink>,
}

impl RecordResumeRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for RecordResumeRunner {
    fn id(&self) -> &str {
        "obs.record.resume"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Resume Recording"
    }

    fn summary(&self) -> &str {
        "Resumes the OBS recording output after a pause."
    }

    fn search_text(&self) -> &str {
        "obs record resume recording continue unpause"
    }

    fn icon_name(&self) -> &str {
        "player-play"
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

        let outcome = SubActionOutcome::from_result(&self.sink.resume_record().await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "obs.record.resume".to_owned(),
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
        let runner = RecordResumeRunner::new(Arc::new(MockSink));
        let (tel, extra) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.record.resume");
        assert!(extra.is_none());
    }
}
