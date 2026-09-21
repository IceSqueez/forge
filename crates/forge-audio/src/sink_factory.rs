use std::sync::Arc;

use async_trait::async_trait;
use forge_types::OutputDevice;

use crate::error::AudioError;
use crate::events::AudioEventSink;
use crate::route::{build_cpal_sink, resolve_output_device};
use crate::sink::AudioSink;

#[async_trait]
pub trait AudioSinkFactory: Send + Sync {
    async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError>;
}

pub struct CpalSinkFactory {
    event_sink: Arc<dyn AudioEventSink>,
}

impl CpalSinkFactory {
    pub fn new(event_sink: Arc<dyn AudioEventSink>) -> Self {
        Self { event_sink }
    }
}

#[async_trait]
impl AudioSinkFactory for CpalSinkFactory {
    async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        let device_id = resolve_output_device(device.clone()).await?;
        Ok(build_cpal_sink(device_id, Arc::clone(&self.event_sink)))
    }
}
