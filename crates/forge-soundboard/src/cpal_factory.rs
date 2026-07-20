use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioSink, CpalSink, DeviceId, NullAudioEventSink, list_output_devices,
    pick_default_output_device,
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
        Ok(Arc::new(CpalSink::new(
            device_id,
            None,
            None,
            Arc::new(NullAudioEventSink),
        )))
    }
}

fn resolve_device_id(device: &OutputDevice) -> Result<DeviceId, AudioError> {
    match device {
        OutputDevice::Default => {
            let devices = list_output_devices()?;
            pick_default_output_device(&devices).ok_or(AudioError::NoDefaultDevice)
        }
        OutputDevice::ByName { name } => {
            let devices = list_output_devices()?;
            if let Some(d) = devices.iter().find(|d| &d.name == name) {
                return Ok(d.id.clone());
            }
            tracing::warn!(
                "output device '{}' not found, falling back to default",
                name
            );
            pick_default_output_device(&devices).ok_or(AudioError::NoDefaultDevice)
        }
        OutputDevice::ById { id } => Ok(DeviceId::new(id)),
    }
}
