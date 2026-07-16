use async_trait::async_trait;
use forge_types::{ClipId, OutputDevice};

#[derive(Debug, thiserror::Error)]
pub enum SoundPlayerError {
    #[error("{0}")]
    Play(String),
}

/// Narrow playback contract used by the action engine.
///
/// Implemented by `forge_soundboard::SoundboardPlayer`. Keeping the trait here
/// breaks the dependency cycle: `forge-runtime` never imports `forge-soundboard`.
///
/// Every method except `play` has a default impl: `stop`/`stop_all` are no-ops
/// returning `Ok`, `set_master_volume` discards the value. A non-bridge impl
/// (the runner test doubles) stays correct without wiring playback state - only
/// `forge_soundboard::SoundboardPlayer` overrides them against a live registry.
#[async_trait]
pub trait SoundPlayer: Send + Sync {
    async fn play(
        &self,
        clip_id: ClipId,
        output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError>;

    /// Halts every in-flight playback of `clip_id`. Stopping a clip that is not
    /// currently playing succeeds without effect.
    async fn stop(&self, clip_id: ClipId) -> Result<(), SoundPlayerError> {
        let _ = clip_id;
        Ok(())
    }

    async fn stop_all(&self) -> Result<(), SoundPlayerError> {
        Ok(())
    }

    /// Sets the linear master gain applied to every subsequent clip at play time.
    async fn set_master_volume(&self, gain: f32) -> Result<(), SoundPlayerError> {
        let _ = gain;
        Ok(())
    }
}
