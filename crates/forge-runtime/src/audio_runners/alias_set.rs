use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
            match config.get(key).and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => {}
                _ => {
                    return Err(RegistryError::UnknownKindId(format!(
                        "tts.alias.set: {key} is required"
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let alias_name = ctx.arg_stack.interpolate(
            config
                .get("alias_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let engine_id = ctx.arg_stack.interpolate(
            config
                .get("engine_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let voice_id = ctx.arg_stack.interpolate(
            config
                .get("voice_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );

        // alias_name serves as both viewer_id (the identity key) and viewer_name
        // (the display name) since the config exposes a single field for both.
        let outcome = match self
            .speak
            .alias_set(alias_name.clone(), alias_name, engine_id, voice_id)
            .await
        {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "tts.alias.set".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
