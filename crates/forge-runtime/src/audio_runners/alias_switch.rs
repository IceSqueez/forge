use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::speak_dispatcher::SpeakDispatcher;

pub struct AliasSwitchRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl AliasSwitchRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for AliasSwitchRunner {
    fn id(&self) -> &str {
        "tts.alias.switch_active"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Switch Active Voice Alias"
    }

    fn summary(&self) -> &str {
        "Repoint an existing viewer alias to a different engine and voice; no-op if the viewer has no alias"
    }

    fn search_text(&self) -> &str {
        "tts alias voice switch change viewer engine"
    }

    fn icon_name(&self) -> &str {
        "refresh-cw"
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
            FormField::DependentSelect {
                key: "voice_id",
                label: "Voice ID",
                options_prefix: "tts.voices",
                depends_on: "engine_id",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        for key in ["alias_name", "engine_id", "voice_id"] {
            if config.str_nonempty(key).is_none() {
                return Err(RegistryError::InvalidConfig(format!(
                    "tts.alias.switch_active: {key} is required"
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
        let timer = StepTimer::start(ctx, "tts.alias.switch_active");

        let alias_name = ctx
            .arg_stack
            .interpolate(config.str("alias_name").unwrap_or_default());
        let engine_id = ctx
            .arg_stack
            .interpolate(config.str("engine_id").unwrap_or_default());
        let voice_id = ctx
            .arg_stack
            .interpolate(config.str("voice_id").unwrap_or_default());

        let result = self
            .speak
            .alias_switch(alias_name, engine_id, voice_id)
            .await;

        (timer.finish(SubActionOutcome::from_result(&result)), None)
    }
}
