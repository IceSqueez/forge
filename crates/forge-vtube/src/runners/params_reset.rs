use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ParamsResetRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ParamsResetRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
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
        "Reset Parameters"
    }

    fn summary(&self) -> &str {
        "Clears all injected parameter values, reverting to face-tracking defaults."
    }

    fn search_text(&self) -> &str {
        "vtube parameter reset clear default face tracking vts"
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

        let outcome = match self.sink.reset_params().await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error::VTubeError;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, Variant};

    struct MockSink;

    #[async_trait]
    impl VTubeSink for MockSink {
        async fn trigger_hotkey(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_expression(&self, _: &str, _: bool) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn set_param(&self, _: &str, _: f64) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn load_model(&self, _: &str) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn reset_params(&self) -> Result<(), VTubeError> {
            Ok(())
        }
        async fn move_model(
            &self,
            _: Option<f64>,
            _: Option<f64>,
            _: Option<f64>,
            _: f64,
        ) -> Result<(), VTubeError> {
            Ok(())
        }
    }

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }

    #[test]
    fn validate_config_accepts_empty_config() {
        let runner = ParamsResetRunner::new(Arc::new(MockSink));
        assert!(runner.validate_config(&BTreeMap::new()).is_ok());
    }

    #[test]
    fn validate_config_accepts_any_config() {
        let runner = ParamsResetRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("extra".to_owned(), Variant::Bool(true))]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[tokio::test]
    async fn execute_succeeds_with_no_config() {
        let runner = ParamsResetRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
    }

    #[tokio::test]
    async fn execute_returns_correct_kind() {
        let runner = ParamsResetRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert_eq!(tel.kind, "vtube.params.reset");
        assert_eq!(tel.outcome, SubActionOutcome::Success);
    }
}
