use std::sync::Arc;

use async_trait::async_trait;
use forge_types::OutputDevice;

use crate::device::cpal_sink::CpalSink;
use crate::device::resolve::resolve_output_device;
use crate::error::AudioError;
use crate::sink::AudioSink;
use forge_speech_types::PcmBuffer;

#[async_trait]
pub trait AudioSinkFactory: Send + Sync {
    async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError>;
}

pub struct CpalSinkFactory;

#[async_trait]
impl AudioSinkFactory for CpalSinkFactory {
    async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        let device_id = resolve_output_device(device.clone()).await?;
        Ok(Arc::new(CpalSink::new(device_id)))
    }
}

pub async fn play_on_default_output(buffer: PcmBuffer) -> Result<(), AudioError> {
    let device_id = resolve_output_device(OutputDevice::Default).await?;
    CpalSink::new(device_id).play(buffer).await
}
