use async_trait::async_trait;

use crate::error::AudioError;
use crate::handle::PlaybackHandle;
use crate::pcm::PcmBuffer;

/// Output target for synthesized or decoded audio.
///
/// Implementations must NOT block — `play` returns once playback is queued/started,
/// not once it completes. Lifecycle events flow over the bus instead (see RFC-038).
#[async_trait]
pub trait AudioSink: Send + Sync {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError>;

    /// Starts playback and returns a token whose `stop` cancels this clip.
    ///
    /// The default forwards to `play` and returns a no-op handle: sinks that cannot
    /// cancel an in-flight clip stay correct (the clip simply runs to completion).
    /// Sinks owning a real device override this to wire the handle to playback.
    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        self.play(buffer).await?;
        Ok(PlaybackHandle::default())
    }
}

/// No-op sink used by the runtime when the soundboard subsystem is not yet wired.
pub struct NullSink;

#[async_trait]
impl AudioSink for NullSink {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }
}
