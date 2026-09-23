use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, ControlledPlayback, PcmBuffer, PlaybackHandle};

/// Reads the installed sink once per playback call, so an install reaches the next clip and leaves a running one untouched.
pub struct RoutedSink {
    current: RwLock<Arc<dyn AudioSink>>,
}

impl RoutedSink {
    pub fn new(initial: Arc<dyn AudioSink>) -> Self {
        Self {
            current: RwLock::new(initial),
        }
    }

    pub fn install(&self, sink: Arc<dyn AudioSink>) {
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = sink;
    }

    fn current(&self) -> Arc<dyn AudioSink> {
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }
}

#[async_trait]
impl AudioSink for RoutedSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        self.current().play(buffer).await
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        self.current().play_stoppable(buffer).await
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.current().play_controlled(buffer).await
    }
}
