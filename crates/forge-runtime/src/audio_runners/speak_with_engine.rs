use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::speak_dispatcher::SpeakDispatcher;

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
        "tts.speak.text_with_engine"
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("text".to_owned(), Variant::String(String::new()));
        cfg.insert("engine_id".to_owned(), Variant::String("piper".to_owned()));
        cfg.insert("wait_for_completion".to_owned(), Variant::Bool(true));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "text",
                label: "Text",
            },
            FormField::DynamicSelect {
                key: "engine_id",
                label: "Engine ID",
                options_key: "tts.engine_ids",
            },
            FormField::Toggle {
                key: "wait_for_completion",
                label: "Wait for completion",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if config.str_nonempty("text").is_none() {
            return Err(RegistryError::InvalidConfig(
                "tts.speak.text_with_engine: text is required".to_owned(),
            ));
        }
        match config.str_nonempty("engine_id") {
            Some(_) => Ok(()),
            None => Err(RegistryError::InvalidConfig(
                "tts.speak.text_with_engine: engine_id is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "tts.speak.text_with_engine");

        let text = ctx
            .arg_stack
            .interpolate(config.str("text").unwrap_or_default());
        let engine_id = ctx
            .arg_stack
            .interpolate(config.str("engine_id").unwrap_or_default());

        let wait_for_completion = config.bool("wait_for_completion").unwrap_or(true);
        let dispatch_result = if wait_for_completion {
            self.speak
                .speak_with_engine_and_wait(text, engine_id, ctx.cancel.clone())
                .await
        } else {
            self.speak.speak_with_engine(text, engine_id).await
        };

        (
            timer.finish(SubActionOutcome::from_result(&dispatch_result)),
            None,
        )
    }
}
