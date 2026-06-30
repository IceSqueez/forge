use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::speak_dispatcher::SpeakDispatcher;

pub struct QueueClearRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl QueueClearRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for QueueClearRunner {
    fn id(&self) -> &str {
        "tts.queue.clear"
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("keep_current".to_owned(), Variant::Bool(true));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Toggle {
            key: "keep_current",
            label: "Let current item finish",
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let keep_current = config
            .get("keep_current")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // When keep_current is false, stop the in-flight item first. The dispatcher
        // exposes no single "clear-all" method; two calls achieve the same effect.
        let outcome = if keep_current {
            match self.speak.clear_keep_current().await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        } else {
            match self.speak.stop_current().await {
                Ok(()) => match self.speak.clear_keep_current().await {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "tts.queue.clear".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
