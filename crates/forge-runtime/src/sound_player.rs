use async_trait::async_trait;
use forge_types::{ClipId, OutputDevice};

#[derive(Debug, thiserror::Error)]
pub enum SoundPlayerError {
    #[error("{0}")]
    Play(String),
}

/// Lives here to avoid a dependency cycle; every method except `play` defaults to a no-op/discard.
#[async_trait]
pub trait SoundPlayer: Send + Sync {
    async fn play(
        &self,
        clip_id: ClipId,
        output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError>;

    /// Stopping a clip that is not currently playing succeeds without effect.
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
