use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use crate::speak_dispatcher::{SpeakDispatchError, SpeakDispatcher};

const QUEUE_CLEAR_KIND_ID: &str = "tts.queue.clear";

const KEEP_CURRENT: ToggleField = ToggleField {
    key: "keep_current",
    label: "Let current item finish",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&KEEP_CURRENT]);

pub struct QueueClearRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl QueueClearRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }

    async fn clear(&self, keep_current: bool) -> Result<(), SpeakDispatchError> {
        if !keep_current {
            self.speak.stop_current().await?;
        }
        self.speak.clear_keep_current().await
    }
}

#[async_trait]
impl SubActionRunner for QueueClearRunner {
    fn id(&self) -> &str {
        QUEUE_CLEAR_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Clear TTS Queue"
    }

    fn summary(&self) -> &str {
        "Drop queued TTS items, optionally also stopping the current utterance"
    }

    fn search_text(&self) -> &str {
        "tts queue clear flush stop all"
    }

    fn icon_name(&self) -> &str {
        "trash-2"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, QUEUE_CLEAR_KIND_ID);
        let keep_current = match KEEP_CURRENT.read(config, ctx.arg_stack) {
            Ok(keep_current) => keep_current,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = crate::speak_dispatcher::dispatch_outcome(&self.clear(keep_current).await);
        (timer.finish(outcome), None)
    }
}
