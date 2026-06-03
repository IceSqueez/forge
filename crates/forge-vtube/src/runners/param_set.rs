use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ParamSetRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ParamSetRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ParamSetRunner {
    fn id(&self) -> &str {
        "vtube.param.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Set Parameter"
    }

    fn summary(&self) -> &str {
        "Injects a value into a VTube Studio parameter."
    }

    fn search_text(&self) -> &str {
        "vtube parameter inject set value face tracking vts"
    }

    fn icon_name(&self) -> &str {
        "sliders"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("param_id".to_owned(), Variant::String(String::new())),
            ("value".to_owned(), Variant::Float(0.0)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "param_id",
                label: "Parameter ID",
                placeholder: "MyCustomParam",
            },
            FormField::Text {
                key: "value",
                label: "Value",
                placeholder: "0.0",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("param_id") {
            Some(Variant::String(_)) => {}
            _ => {
                return Err(RegistryError::UnknownKindId(
                    "vtube.param.set: 'param_id' must be a string".to_owned(),
                ));
            }
        }
        match config.get("value") {
            Some(Variant::Float(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "vtube.param.set: 'value' must be a float".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_id = config
            .get("param_id")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let param_id = ctx.arg_stack.interpolate(raw_id);

        let value = match config.get("value") {
            Some(Variant::Float(f)) => *f,
            _ => 0.0,
        };

        let outcome = match self.sink.set_param(&param_id, value).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                kind: "vtube.param.set".to_owned(),
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
    use forge_types::EventId;

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
    fn validate_config_accepts_valid_param() {
        let runner = ParamSetRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("MyParam".to_owned())),
            ("value".to_owned(), Variant::Float(0.5)),
        ]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_param_id() {
        let runner = ParamSetRunner::new(Arc::new(MockSink));
        let config = BTreeMap::from([("value".to_owned(), Variant::Float(1.0))]);
        assert!(runner.validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn execute_interpolates_param_id_not_value() {
        let runner = ParamSetRunner::new(Arc::new(MockSink));
        let stack =
            ArgStack::new().set("pid".to_owned(), Variant::String("DynamicParam".to_owned()));
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("%pid%".to_owned())),
            ("value".to_owned(), Variant::Float(0.75)),
        ]);
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
    }

    #[tokio::test]
    async fn execute_returns_success_on_mock_sink() {
        let runner = ParamSetRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let config = BTreeMap::from([
            ("param_id".to_owned(), Variant::String("ParamA".to_owned())),
            ("value".to_owned(), Variant::Float(1.0)),
        ]);
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "vtube.param.set");
    }
}
