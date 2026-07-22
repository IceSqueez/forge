use async_trait::async_trait;

use crate::error::AudioError;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;

/// Implementations must NOT block - `play` returns once playback is queued/started,
/// not once it completes.
#[async_trait]
pub trait AudioSink: Send + Sync {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError>;

    /// Default degrades to a no-op handle; sinks that cannot cancel an in-flight clip
    /// stay correct by letting it run to completion.
    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        self.play(buffer).await?;
        Ok(PlaybackHandle::default())
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.play(buffer).await?;
        Ok(ControlledPlayback::completed())
    }
}

pub struct NullSink;

#[async_trait]
impl AudioSink for NullSink {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }
}
