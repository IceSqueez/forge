use std::sync::Arc;

use forge_types::{ArgStack, SubActionOutcome, SubActionSpec, SubActionTelemetry};
use time::OffsetDateTime;

use crate::sound_player::SoundPlayer;

pub(super) async fn run(
    spec: &SubActionSpec,
    index: usize,
    player: Option<&Arc<dyn SoundPlayer>>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let kind = spec.kind_label().to_string();
    let started_at = OffsetDateTime::now_utc();

    let Some(player) = player else {
        return (
            SubActionTelemetry {
                index,
                kind,
                started_at,
                duration_ms: 0,
                outcome: SubActionOutcome::Skipped("soundboard subsystem unavailable".to_string()),
            },
            None,
        );
    };

    let SubActionSpec::PlaySound {
        clip_id,
        output_device_override,
    } = spec
    else {
        unreachable!("play_sound::run called with non-PlaySound spec")
    };

    let result = player.play(*clip_id, output_device_override.clone()).await;

    let duration_ms = (OffsetDateTime::now_utc() - started_at)
        .whole_milliseconds()
        .max(0) as u64;

    let outcome = match result {
        Ok(()) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    };

    (
        SubActionTelemetry {
            index,
            kind,
            started_at,
            duration_ms,
            outcome,
        },
        None,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use async_trait::async_trait;
    use forge_types::{ClipId, OutputDevice, SubActionOutcome, SubActionSpec};

    use super::*;
    use crate::sound_player::{SoundPlayer, SoundPlayerError};

    struct AlwaysOkPlayer;

    #[async_trait]
    impl SoundPlayer for AlwaysOkPlayer {
        async fn play(
            &self,
            _clip_id: ClipId,
            _override_device: Option<OutputDevice>,
        ) -> Result<(), SoundPlayerError> {
            Ok(())
        }
    }

    struct AlwaysFailPlayer;

    #[async_trait]
    impl SoundPlayer for AlwaysFailPlayer {
        async fn play(
            &self,
            _clip_id: ClipId,
            _override_device: Option<OutputDevice>,
        ) -> Result<(), SoundPlayerError> {
            Err(SoundPlayerError::Play("clip not found".to_string()))
        }
    }

    fn play_sound_spec() -> SubActionSpec {
        SubActionSpec::PlaySound {
            clip_id: ClipId::new(),
            output_device_override: None,
        }
    }

    #[tokio::test]
    async fn none_player_returns_skipped() {
        let spec = play_sound_spec();
        let (telemetry, updated) = run(&spec, 0, None).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Skipped(_)));
        assert_eq!(telemetry.kind, "PlaySound");
        assert_eq!(telemetry.index, 0);
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn ok_player_returns_success() {
        let spec = play_sound_spec();
        let player: Arc<dyn SoundPlayer> = Arc::new(AlwaysOkPlayer);
        let (telemetry, updated) = run(&spec, 1, Some(&player)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        assert_eq!(telemetry.index, 1);
        assert!(updated.is_none());
    }

    #[tokio::test]
    async fn fail_player_returns_failed() {
        let spec = play_sound_spec();
        let player: Arc<dyn SoundPlayer> = Arc::new(AlwaysFailPlayer);
        let (telemetry, _) = run(&spec, 2, Some(&player)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(telemetry.index, 2);
    }

    #[tokio::test]
    async fn device_override_is_forwarded() {
        use std::sync::{Arc, Mutex};

        struct CapturingPlayer {
            captured: Arc<Mutex<Option<OutputDevice>>>,
        }

        #[async_trait]
        impl SoundPlayer for CapturingPlayer {
            async fn play(
                &self,
                _clip_id: ClipId,
                override_device: Option<OutputDevice>,
            ) -> Result<(), SoundPlayerError> {
                *self.captured.lock().unwrap() = override_device;
                Ok(())
            }
        }

        let captured: Arc<Mutex<Option<OutputDevice>>> = Arc::new(Mutex::new(None));
        let player: Arc<dyn SoundPlayer> = Arc::new(CapturingPlayer {
            captured: Arc::clone(&captured),
        });

        let override_dev = OutputDevice::ByName {
            name: "Headphones".to_string(),
        };
        let spec = SubActionSpec::PlaySound {
            clip_id: ClipId::new(),
            output_device_override: Some(override_dev.clone()),
        };

        run(&spec, 0, Some(&player)).await;

        assert_eq!(*captured.lock().unwrap(), Some(override_dev));
    }
}
