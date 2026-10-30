use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, NumberField, OptionalField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use crate::runners::fields::optional_number;
use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const DEFAULT_OVERRIDE_SECONDS: f64 = 2.0;
const FULL_STRENGTH: f64 = 1.0;
const NO_STRENGTH: f64 = 0.0;

const ENABLED: ToggleField = ToggleField {
    key: config_keys::ENABLED,
    label: "Physics Enabled",
    default: true,
};

const OVERRIDE_DURATION: OptionalField<NumberField> =
    optional_number(config_keys::DURATION, "Duration (s)", "0.5 to 5");

const CONFIG: ConfigSpec = ConfigSpec::new(&[&ENABLED, &OVERRIDE_DURATION]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(bool, Option<f64>), ConfigProblem> {
    Ok((
        ENABLED.read(config, args)?,
        OVERRIDE_DURATION.read(config, args)?,
    ))
}

pub struct ModelSetPhysicsRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ModelSetPhysicsRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ModelSetPhysicsRunner {
    fn id(&self) -> &str {
        sub_action_ids::MODEL_SET_PHYSICS
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (enabled, duration) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let strength = if enabled { FULL_STRENGTH } else { NO_STRENGTH };
        let outcome = step_failure::outcome(
            &self
                .sink
                .set_physics_override(strength, duration.unwrap_or(DEFAULT_OVERRIDE_SECONDS))
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
