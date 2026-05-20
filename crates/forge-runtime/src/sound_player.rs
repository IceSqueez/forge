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
#[async_trait]
pub trait SoundPlayer: Send + Sync {
    async fn play(
        &self,
        clip_id: ClipId,
        output_device_override: Option<OutputDevice>,
    ) -> Result<(), SoundPlayerError>;
}
