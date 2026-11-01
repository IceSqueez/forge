use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use crate::speech::speak_dispatcher::SpeakQueueControl;

const QUEUE_PAUSE_KIND_ID: &str = "tts.queue.pause";

pub(crate) struct QueuePauseRunner {
    speak: Arc<dyn SpeakQueueControl>,
}

impl QueuePauseRunner {
    pub(crate) fn new(speak: Arc<dyn SpeakQueueControl>) -> Self {
        Self { speak }
    }
}

impl StepCatalogEntry for QueuePauseRunner {
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
}

#[async_trait]
impl SubActionRunner for QueuePauseRunner {
    fn id(&self) -> &str {
        QUEUE_PAUSE_KIND_ID
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
        let outcome = crate::speech::speak_dispatcher::dispatch_outcome(&self.speak.pause().await);
        (timer.finish(outcome), None)
    }
}
