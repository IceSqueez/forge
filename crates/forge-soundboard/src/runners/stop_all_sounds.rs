use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use crate::control::SoundboardControl;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::SOUND_STOP_ALL;

pub struct StopAllSoundsRunner {
    control: Arc<dyn SoundboardControl>,
}

impl StopAllSoundsRunner {
    pub fn new(control: Arc<dyn SoundboardControl>) -> Self {
        Self { control }
    }
}

#[async_trait]
impl SubActionRunner for StopAllSoundsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Stop All Sounds"
    }

    fn summary(&self) -> &str {
        "Stop every soundboard clip currently playing"
    }

    fn search_text(&self) -> &str {
        "stop all sounds clips audio soundboard silence"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let outcome = SubActionOutcome::from_result(&self.control.stop_all().await);
        (timer.finish(outcome), None)
    }
}
