use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, DynamicSelectField, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

use super::{ENGINE, SPEECH_TEXT, WAIT_FOR_COMPLETION};
use crate::speak_dispatcher::{SpeakDispatcher, SpeechOrigin};

const SPEAK_WITH_ENGINE_KIND_ID: &str = "tts.speak.text_with_engine";
const DEFAULT_ENGINE_ID: &str = "piper";

const ENGINE_WITH_DEFAULT: DynamicSelectField<String> = ENGINE.with_default(DEFAULT_ENGINE_ID);

const CONFIG: ConfigSpec =
    ConfigSpec::new(&[&SPEECH_TEXT, &ENGINE_WITH_DEFAULT, &WAIT_FOR_COMPLETION]);

struct EngineSpeech {
    text: String,
    engine_id: String,
    wait_for_completion: bool,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<EngineSpeech, ConfigProblem> {
    Ok(EngineSpeech {
        text: SPEECH_TEXT.read(config, args)?,
        engine_id: ENGINE_WITH_DEFAULT.read(config, args)?,
        wait_for_completion: WAIT_FOR_COMPLETION.read(config, args)?,
    })
}

pub struct SpeakWithEngineRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl SpeakWithEngineRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for SpeakWithEngineRunner {
    fn id(&self) -> &str {
        SPEAK_WITH_ENGINE_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Speak with Engine Override"
    }

    fn summary(&self) -> &str {
        "Queue a TTS utterance using a specific engine"
    }

    fn search_text(&self) -> &str {
        "speak tts text engine override queue"
    }

    fn icon_name(&self) -> &str {
        "cpu"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, SPEAK_WITH_ENGINE_KIND_ID);
        let EngineSpeech {
            text,
            engine_id,
            wait_for_completion,
        } = match read_config(config, ctx.arg_stack) {
            Ok(speech) => speech,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let origin = SpeechOrigin::from_args(ctx.arg_stack, Some(ctx.parent_event_id));
        let dispatch_result = if wait_for_completion {
            self.speak
                .speak_with_engine_and_wait(text, engine_id, origin, ctx.cancel.clone())
                .await
        } else {
            self.speak.speak_with_engine(text, engine_id, origin).await
        };

        (
            timer.finish(crate::speak_dispatcher::dispatch_outcome(&dispatch_result)),
            None,
        )
    }
}
