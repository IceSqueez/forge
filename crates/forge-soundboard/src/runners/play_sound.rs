use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField,
};
use forge_types::{ArgStack, ClipId, SubActionConfig, SubActionTelemetry};

use super::clip_field::ClipField;
use crate::control::SoundboardControl;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::SOUND_PLAY;

const CLIP: ClipField = ClipField { required: true };

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CLIP]);

fn read_clip(config: &SubActionConfig, args: &ArgStack) -> Result<ClipId, ConfigProblem> {
    CLIP.read(config, args)?
        .ok_or_else(|| ConfigProblem::field(config_keys::CLIP_ID, ConfigReason::Required))
}

pub struct PlaySoundRunner {
    control: Arc<dyn SoundboardControl>,
}

impl PlaySoundRunner {
    pub fn new(control: Arc<dyn SoundboardControl>) -> Self {
        Self { control }
    }
}

#[async_trait]
impl SubActionRunner for PlaySoundRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Play Sound"
    }

    fn summary(&self) -> &str {
        "Play a soundboard clip on the configured output device"
    }

    fn search_text(&self) -> &str {
        "play sound clip audio soundboard"
    }

    fn icon_name(&self) -> &str {
        "volume"
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
        let clip_id = match read_clip(config, ctx.arg_stack) {
            Ok(clip_id) => clip_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = super::step_failure::outcome(&self.control.play(clip_id, None).await);
        (timer.finish(outcome), None)
    }
}
