use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink};
use forge_types::OutputDevice;

#[async_trait]
pub trait AudioSinkFactory: Send + Sync {
    async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError>;
}
