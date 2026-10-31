use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::STUDIO_SET_ENABLED;

const ON: ToggleField = ToggleField {
    key: config_keys::ON,
    label: "Enabled",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&ON]);

pub(crate) struct StudioSetEnabledRunner {
    sink: Arc<dyn ObsSink>,
}

impl StudioSetEnabledRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for StudioSetEnabledRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Studio Mode"
    }

    fn summary(&self) -> &str {
        "Sets OBS studio mode to an explicit on or off state."
    }

    fn search_text(&self) -> &str {
        "obs studio mode set on off enable disable preview"
    }

    fn icon_name(&self) -> &str {
        "adjustments-horizontal"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let on = match ON.read(config, ctx.arg_stack) {
            Ok(on) => on,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.set_studio_mode(on).await);
        (timer.finish(outcome), None)
    }
}
