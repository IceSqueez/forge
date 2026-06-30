use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::sound_player::SoundPlayer;

pub struct StopAllSoundsRunner {
    sound_player: Arc<dyn SoundPlayer>,
}

impl StopAllSoundsRunner {
    pub fn new(sound_player: Arc<dyn SoundPlayer>) -> Self {
        Self { sound_player }
    }
}

#[async_trait]
impl SubActionRunner for StopAllSoundsRunner {
    fn id(&self) -> &str {
        "soundboard.sound.stop_all"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Stop All Sounds"
    }

    fn summary(&self) -> &str {
        "Stop every soundboard clip currently playing"
    }

    fn search_text(&self) -> &str {
        "stop all sounds clips audio soundboard silence"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
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

        let outcome = match self.sound_player.stop_all().await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "soundboard.sound.stop_all".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
