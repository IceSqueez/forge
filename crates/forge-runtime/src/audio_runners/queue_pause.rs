use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::speak_dispatcher::SpeakDispatcher;

pub struct QueuePauseRunner {
    speak: Arc<dyn SpeakDispatcher>,
}

impl QueuePauseRunner {
    pub fn new(speak: Arc<dyn SpeakDispatcher>) -> Self {
        Self { speak }
    }
}

#[async_trait]
impl SubActionRunner for QueuePauseRunner {
    fn id(&self) -> &str {
        "tts.queue.pause"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Tts
    }

    fn label(&self) -> &str {
        "Pause TTS Queue"
    }

    fn summary(&self) -> &str {
        "Pause processing of the TTS speak queue"
    }

    fn search_text(&self) -> &str {
        "tts queue pause hold"
    }

    fn icon_name(&self) -> &str {
        "pause-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let outcome = match self.speak.pause().await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "tts.queue.pause".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
