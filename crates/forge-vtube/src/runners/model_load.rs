use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, PickedTextField, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};

const MODEL_ID: PickedTextField = PickedTextField {
    key: config_keys::MODEL_ID,
    label: "Model",
    picker: TextPicker::Options {
        options_key: picker_keys::MODEL_IDS,
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MODEL_ID]);

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
        sub_action_ids::MODEL_LOAD
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let model_id = match MODEL_ID.read(config, ctx.arg_stack) {
            Ok(model_id) => model_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(&self.sink.load_model(&model_id).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
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
