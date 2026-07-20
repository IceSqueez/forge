use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioSink, DeviceId, NullAudioEventSink, build_cpal_sink, list_output_devices,
    resolve_device,
};
use forge_types::OutputDevice;

use crate::sink_factory::AudioSinkFactory;

pub struct CpalSinkFactory;

#[async_trait]
impl AudioSinkFactory for CpalSinkFactory {
    async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
        let device = device.clone();
        let device_id = tokio::task::spawn_blocking(move || resolve_device_id(&device))
            .await
            .map_err(|e| AudioError::Host(e.to_string()))??;
        Ok(build_cpal_sink(device_id, Arc::new(NullAudioEventSink)))
    }
}

fn resolve_device_id(device: &OutputDevice) -> Result<DeviceId, AudioError> {
    let devices = list_output_devices()?;
    resolve_device(device, &devices).ok_or(AudioError::NoDefaultDevice)
}
