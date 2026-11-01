pub(crate) mod fan_out;
pub(crate) mod playback;
pub(crate) mod route;

use async_trait::async_trait;

use crate::error::AudioError;
use forge_speech_types::PcmBuffer;
use playback::{ControlledPlayback, PlaybackControl};

#[async_trait]
pub trait AudioSink: Send + Sync {
    async fn start(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError>;

    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        self.start(buffer).await?.await
    }
}

pub struct NullSink;

#[async_trait]
impl AudioSink for NullSink {
    async fn start(&self, _buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        Ok(ControlledPlayback::new(
            PlaybackControl::new(),
            std::future::ready(Ok(())),
        ))
    }
}
