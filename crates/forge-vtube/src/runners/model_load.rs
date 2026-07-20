use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ModelLoadRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelLoadRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ModelLoadRunner {
    fn id(&self) -> &str {
        "vtube.model.load"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Load Model"
    }

    fn summary(&self) -> &str {
        "Loads a VTube Studio model by its ID."
    }

    fn search_text(&self) -> &str {
        "vtube model load avatar character vts"
    }

    fn icon_name(&self) -> &str {
        "user"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("model_id".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "model_id",
            label: "Model",
            options_key: "vtube.model_ids",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("model_id") {
            Some(Variant::String(_)) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "vtube.model.load: 'model_id' must be a string".to_owned(),
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

        let raw = config
            .get("model_id")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let model_id = ctx.arg_stack.interpolate(raw);

        let outcome = match self.sink.load_model(&model_id).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.model.load".to_owned(),
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

    #[test]
    fn validate_config_accepts_model_id_string() {
        let runner = ModelLoadRunner::new(Arc::new(MockSink::new()));
        let config = BTreeMap::from([(
            "model_id".to_owned(),
            Variant::String("model-001".to_owned()),
        )]);
        assert!(runner.validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_missing_model_id() {
        let runner = ModelLoadRunner::new(Arc::new(MockSink::new()));
        assert!(runner.validate_config(&BTreeMap::new()).is_err());
    }

    #[tokio::test]
    async fn execute_interpolates_model_id() {
        let runner = ModelLoadRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new().set("mid".to_owned(), Variant::String("model-abc".to_owned()));
        let config = BTreeMap::from([("model_id".to_owned(), Variant::String("%mid%".to_owned()))]);
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert!(extra.is_none());
    }

    #[tokio::test]
    async fn execute_returns_success_on_mock_sink() {
        let runner = ModelLoadRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let config = BTreeMap::from([(
            "model_id".to_owned(),
            Variant::String("model-xyz".to_owned()),
        )]);
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&config, &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "vtube.model.load");
    }
}
