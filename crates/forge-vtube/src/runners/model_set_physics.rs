use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ModelSetPhysicsRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelSetPhysicsRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

fn read_opt_float(config: &SubActionConfig, key: &str) -> Option<f64> {
    match config.get(key) {
        Some(Variant::Float(f)) => Some(*f),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for ModelSetPhysicsRunner {
    fn id(&self) -> &str {
        "vtube.model.set_physics"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Toggle Physics"
    }

    fn summary(&self) -> &str {
        "Temporarily overrides the VTube Studio model's physics strength."
    }

    fn search_text(&self) -> &str {
        "vtube model physics toggle strength override wind vts"
    }

    fn icon_name(&self) -> &str {
        "activity"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("enabled".to_owned(), Variant::Bool(true))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Toggle {
                key: "enabled",
                label: "Physics Enabled",
            },
            FormField::Optional {
                key: "duration",
                label: "Duration (s)",
                inner: Box::new(FormField::Text {
                    key: "duration",
                    label: "Duration (s)",
                    placeholder: "0.5 to 5",
                }),
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("enabled") {
            Some(Variant::Bool(_)) | None => Ok(()),
            _ => Err(RegistryError::InvalidConfig(
                "vtube.model.set_physics: 'enabled' must be a bool".to_owned(),
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

        let enabled = !matches!(config.get("enabled"), Some(Variant::Bool(false)));
        let strength = if enabled { 1.0 } else { 0.0 };
        let override_seconds = read_opt_float(config, "duration").unwrap_or(2.0);

        let outcome = SubActionOutcome::from_result(
            &self
                .sink
                .set_physics_override(strength, override_seconds)
                .await,
        );

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.model.set_physics".to_owned(),
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
    async fn execute_default_config_enables_physics() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelSetPhysicsRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&runner.default_config(), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_disabled_still_calls_sink() {
        let sink = Arc::new(MockSink::new());
        let runner = ModelSetPhysicsRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let config = BTreeMap::from([("enabled".to_owned(), Variant::Bool(false))]);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_propagates_sink_error() {
        let runner = ModelSetPhysicsRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
    }

    #[test]
    fn validate_config_accepts_missing_enabled() {
        let runner = ModelSetPhysicsRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_ok());
    }
}
