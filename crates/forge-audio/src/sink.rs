use async_trait::async_trait;

use crate::error::AudioError;
use crate::pcm::PcmBuffer;

/// Output target for synthesized or decoded audio.
///
/// Implementations must NOT block — `play` returns once playback is queued/started,
/// not once it completes. Lifecycle events flow over the bus instead (see RFC-038).
#[async_trait]
pub trait AudioSink: Send + Sync {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError>;
}

/// No-op sink used by the runtime when the soundboard subsystem is not yet wired.
pub struct NullSink;

#[async_trait]
impl AudioSink for NullSink {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }
}
