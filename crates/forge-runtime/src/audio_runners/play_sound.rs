use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, ClipId, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sound_player::SoundPlayer;

pub struct PlaySoundRunner {
    sound_player: Arc<dyn SoundPlayer>,
}

impl PlaySoundRunner {
    pub fn new(sound_player: Arc<dyn SoundPlayer>) -> Self {
        Self { sound_player }
    }
}

#[async_trait]
impl SubActionRunner for PlaySoundRunner {
    fn id(&self) -> &str {
        "soundboard.sound.play"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Audio
    }

    fn label(&self) -> &str {
        "Play Sound"
    }

    fn summary(&self) -> &str {
        "Play a soundboard clip on the configured output device"
    }

    fn search_text(&self) -> &str {
        "play sound clip audio soundboard"
    }

    fn icon_name(&self) -> &str {
        "volume"
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

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("clip_id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "soundboard.sound.play: clip_id is required".to_owned(),
            )),
        }
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

        let parse_result: Result<ClipId, _> =
            serde_json::from_str(&format!("\"{interpolated}\""));

        let outcome = match parse_result {
            Err(_) => SubActionOutcome::Failed(format!(
                "invalid clip_id: {interpolated}"
            )),
            Ok(clip_id) => match self.sound_player.play(clip_id, None).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "soundboard.sound.play".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use async_trait::async_trait;
    use forge_types::{ClipId, EventId, OutputDevice, SubActionOutcome};

    use super::*;
    use crate::sound_player::{SoundPlayer, SoundPlayerError};
    use forge_events::{Event, EventPublisher};

    struct NullPublisher;

    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    struct OkPlayer;

    #[async_trait]
    impl SoundPlayer for OkPlayer {
        async fn play(
            &self,
            _clip_id: ClipId,
            _override_device: Option<OutputDevice>,
        ) -> Result<(), SoundPlayerError> {
            Ok(())
        }
    }

    struct FailPlayer;

    #[async_trait]
    impl SoundPlayer for FailPlayer {
        async fn play(
            &self,
            _clip_id: ClipId,
            _override_device: Option<OutputDevice>,
        ) -> Result<(), SoundPlayerError> {
            Err(SoundPlayerError::Play("not found".to_owned()))
        }
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        }
    }

    fn config_with_clip(id: &ClipId) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "clip_id".to_owned(),
            Variant::String(id.to_string()),
        );
        cfg
    }

    #[tokio::test]
    async fn success_path() {
        let runner = PlaySoundRunner::new(Arc::new(OkPlayer));
        let clip_id = ClipId::new();
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, updated) = runner.execute(&config_with_clip(&clip_id), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn failure_path() {
        let runner = PlaySoundRunner::new(Arc::new(FailPlayer));
        let clip_id = ClipId::new();
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&config_with_clip(&clip_id), &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn invalid_clip_id_returns_failed() {
        let runner = PlaySoundRunner::new(Arc::new(OkPlayer));
        let mut cfg = SubActionConfig::new();
        cfg.insert("clip_id".to_owned(), Variant::String("not-a-ulid".to_owned()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn empty_clip_id_returns_failed() {
        let runner = PlaySoundRunner::new(Arc::new(OkPlayer));
        let cfg = runner.default_config();
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (telemetry, _) = runner.execute(&cfg, &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
    }

    #[test]
    fn validate_config_rejects_missing_clip_id() {
        let runner = PlaySoundRunner::new(Arc::new(OkPlayer));
        assert!(runner.validate_config(&SubActionConfig::new()).is_err());
    }

    #[test]
    fn validate_config_accepts_nonempty_clip_id() {
        let runner = PlaySoundRunner::new(Arc::new(OkPlayer));
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "clip_id".to_owned(),
            Variant::String(ClipId::new().to_string()),
        );
        assert!(runner.validate_config(&cfg).is_ok());
    }
}
