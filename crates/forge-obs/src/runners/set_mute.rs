use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, PickedTextField, RunContext, StepCatalogEntry, StepTimer,
    SubActionCategory, SubActionRunner, TextPicker, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, picker_keys, sub_action_ids};

const SOURCE: PickedTextField = PickedTextField {
    key: config_keys::SOURCE,
    label: "Input",
    picker: TextPicker::Options {
        options_key: picker_keys::INPUT_NAMES,
    },
    default: "",
    required: true,
};

const MUTED: ToggleField = ToggleField {
    key: config_keys::MUTED,
    label: "Muted",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &MUTED]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, bool), ConfigProblem> {
    Ok((SOURCE.read(config, args)?, MUTED.read(config, args)?))
}

pub(crate) struct SetMuteRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetMuteRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

impl StepCatalogEntry for SetMuteRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
    }
    fn label(&self) -> &str {
        "Set Input Mute"
    }
    fn summary(&self) -> &str {
        "Mutes or unmutes an OBS audio input."
    }
    fn search_text(&self) -> &str {
        "obs audio mute unmute input source volume"
    }
    fn icon_name(&self) -> &str {
        "volume"
    }
}

#[async_trait]
impl SubActionRunner for SetMuteRunner {
    fn id(&self) -> &str {
        sub_action_ids::AUDIO_SET_MUTE
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
        let (source, muted) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.set_input_mute(&source, muted).await);
        (timer.finish(outcome), None)
    }
}
