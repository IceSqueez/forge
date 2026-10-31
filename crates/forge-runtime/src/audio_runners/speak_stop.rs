use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use crate::speak_dispatcher::SpeakDispatcher;

const SPEAK_STOP_KIND_ID: &str = "tts.speak.stop";

pub(crate) struct SpeakStopRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl SpeakStopRunner {
    pub(crate) fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for SpeakStopRunner {
    fn id(&self) -> &str {
        SPEAK_STOP_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Stop Speaking"
    }

    fn summary(&self) -> &str {
        "Interrupt the current TTS utterance"
    }

    fn search_text(&self) -> &str {
        "tts stop speaking interrupt current"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ConfigSpec::new(&[]))
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, SPEAK_STOP_KIND_ID);
        let outcome = crate::speak_dispatcher::dispatch_outcome(&self.speak.stop_current().await);
        (timer.finish(outcome), None)
    }
}
