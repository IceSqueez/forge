use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use crate::speak_dispatcher::SpeakDispatcher;

const QUEUE_RESUME_KIND_ID: &str = "tts.queue.resume";

pub struct QueueResumeRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl QueueResumeRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for QueueResumeRunner {
    fn id(&self) -> &str {
        QUEUE_RESUME_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Resume TTS Queue"
    }

    fn summary(&self) -> &str {
        "Resume processing of the TTS speak queue after a pause"
    }

    fn search_text(&self) -> &str {
        "tts queue resume unpause continue"
    }

    fn icon_name(&self) -> &str {
        "play-circle"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ConfigSpec::new(&[]))
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, QUEUE_RESUME_KIND_ID);
        let outcome = SubActionOutcome::from_result(&self.speak.resume().await);
        (timer.finish(outcome), None)
    }
}
