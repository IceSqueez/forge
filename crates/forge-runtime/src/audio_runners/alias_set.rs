use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::speak_dispatcher::SpeakDispatcher;

pub struct AliasSetRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl AliasSetRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for AliasSetRunner {
    fn id(&self) -> &str {
        "tts.alias.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Set Voice Alias"
    }

    fn summary(&self) -> &str {
        "Assign a specific engine and voice to a viewer alias"
    }

    fn search_text(&self) -> &str {
        "tts alias voice set assign viewer engine"
    }

    fn icon_name(&self) -> &str {
        "user-check"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("alias_name".to_owned(), Variant::String(String::new()));
        cfg.insert("engine_id".to_owned(), Variant::String(String::new()));
        cfg.insert("voice_id".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "alias_name",
                label: "Alias name",
                placeholder: "e.g. %user% or viewer id",
            },
            FormField::DynamicSelect {
                key: "engine_id",
                label: "Engine ID",
                options_key: "tts.engine_ids",
            },
            FormField::Text {
                key: "voice_id",
                label: "Voice ID",
                placeholder: "e.g. en_US-amy-medium",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        for key in ["alias_name", "engine_id", "voice_id"] {
            if config.str_nonempty(key).is_none() {
                return Err(RegistryError::InvalidConfig(format!(
                    "tts.alias.set: {key} is required"
                )));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "tts.alias.set");

        let alias_name = ctx
            .arg_stack
            .interpolate(config.str("alias_name").unwrap_or_default());
        let engine_id = ctx
            .arg_stack
            .interpolate(config.str("engine_id").unwrap_or_default());
        let voice_id = ctx
            .arg_stack
            .interpolate(config.str("voice_id").unwrap_or_default());

        // alias_name serves as both viewer_id (the identity key) and viewer_name
        // (the display name) since the config exposes a single field for both.
        let result = self
            .speak
            .alias_set(alias_name.clone(), alias_name, engine_id, voice_id)
            .await;

        (timer.finish(SubActionOutcome::from_result(&result)), None)
    }
}
