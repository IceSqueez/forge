use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use crate::speak_dispatcher::SpeakDispatcher;

const QUEUE_PAUSE_KIND_ID: &str = "tts.queue.pause";

pub struct QueuePauseRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl QueuePauseRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for QueuePauseRunner {
    fn id(&self) -> &str {
        QUEUE_PAUSE_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Pause TTS Queue"
    }

    fn summary(&self) -> &str {
        "Pause processing of the TTS speak queue"
    }

    fn search_text(&self) -> &str {
        "tts queue pause hold"
    }

    fn icon_name(&self) -> &str {
        "pause-circle"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ConfigSpec::new(&[]))
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, QUEUE_PAUSE_KIND_ID);
        let outcome = SubActionOutcome::from_result(&self.speak.pause().await);
        (timer.finish(outcome), None)
    }
}
