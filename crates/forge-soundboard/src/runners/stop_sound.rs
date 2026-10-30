use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use super::clip_field::ClipField;
use crate::control::SoundboardControl;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::SOUND_STOP;

const CLIP: ClipField = ClipField { required: false };

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CLIP]);

pub struct StopSoundRunner {
    control: Arc<dyn SoundboardControl>,
}

impl StopSoundRunner {
    pub fn new(control: Arc<dyn SoundboardControl>) -> Self {
        Self { control }
    }
}

#[async_trait]
impl SubActionRunner for StopSoundRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Stop Sound"
    }

    fn summary(&self) -> &str {
        "Stop a soundboard clip (leave the clip empty to stop everything)"
    }

    fn search_text(&self) -> &str {
        "stop sound clip audio soundboard halt"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
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
        let stopped = match CLIP.read(config, ctx.arg_stack) {
            Ok(Some(clip_id)) => self.control.stop(clip_id).await,
            Ok(None) => self.control.stop_all().await,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(super::step_failure::outcome(&stopped)), None)
    }
}
