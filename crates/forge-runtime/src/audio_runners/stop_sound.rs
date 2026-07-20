use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{
    ArgStack, ClipId, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

use crate::sound_player::SoundPlayer;

pub struct StopSoundRunner {
    sound_player: Arc<dyn SoundPlayer>,
}

impl StopSoundRunner {
    pub fn new(sound_player: Arc<dyn SoundPlayer>) -> Self {
        Self { sound_player }
    }
}

#[async_trait]
impl SubActionRunner for StopSoundRunner {
    fn id(&self) -> &str {
        "soundboard.sound.stop"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Stop Sound"
    }

    fn summary(&self) -> &str {
        "Stop a soundboard clip (leave the clip empty to stop everything)"
    }

    fn search_text(&self) -> &str {
        "stop sound clip audio soundboard halt"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("clip_id".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "clip_id",
            label: "Clip",
            options_key: "soundboard.clip_ids",
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

        let clip_id_str = config
            .get("clip_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let interpolated = ctx.arg_stack.interpolate(clip_id_str);

        let outcome = if interpolated.is_empty() {
            match self.sound_player.stop_all().await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        } else {
            let parse_result: Result<ClipId, _> =
                serde_json::from_str(&format!("\"{interpolated}\""));
            match parse_result {
                Err(_) => SubActionOutcome::Failed(format!("invalid clip_id: {interpolated}")),
                Ok(clip_id) => match self.sound_player.stop(clip_id).await {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "soundboard.sound.stop".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
