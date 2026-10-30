use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use crate::speak_dispatcher::SpeakDispatcher;

const QUEUE_SKIP_KIND_ID: &str = "tts.queue.skip_current";

pub struct QueueSkipRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl QueueSkipRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for QueueSkipRunner {
    fn id(&self) -> &str {
        QUEUE_SKIP_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Skip Current TTS"
    }

    fn summary(&self) -> &str {
        "Skip the current TTS item and advance to the next in the queue"
    }

    fn search_text(&self) -> &str {
        "tts queue skip next advance"
    }

    fn icon_name(&self) -> &str {
        "skip-forward"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ConfigSpec::new(&[]))
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, QUEUE_SKIP_KIND_ID);
        let outcome = crate::speak_dispatcher::dispatch_outcome(&self.speak.skip_current().await);
        (timer.finish(outcome), None)
    }
}
