use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("text").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(
                    "tts.speak.text_with_engine: text is required".to_owned(),
                ));
            }
        }
        match config.get("engine_id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "tts.speak.text_with_engine: engine_id is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let text = ctx.arg_stack.interpolate(
            config
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );
        let engine_id = ctx.arg_stack.interpolate(
            config
                .get("engine_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
        );

        let outcome = match self.speak.speak_with_engine(text, engine_id).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "tts.speak.text_with_engine".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
