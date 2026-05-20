use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioSink, CpalSink, DeviceId, NullAudioEventSink, list_output_devices,
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
            devices
                .into_iter()
                .find(|d| d.is_default)
                .or_else(|| list_output_devices().ok()?.into_iter().next())
                .map(|d| d.id)
                .ok_or(AudioError::NoDefaultDevice)
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
            devices
                .into_iter()
                .find(|d| d.is_default)
                .or_else(|| list_output_devices().ok()?.into_iter().next())
                .map(|d| d.id)
                .ok_or(AudioError::NoDefaultDevice)
        }
        OutputDevice::ById { id } => Ok(DeviceId::new(id)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires audio host (cpal) — run manually with `cargo test -- --ignored`"]
    fn cpal_factory_builds_sink_for_default_device() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let factory = CpalSinkFactory;
            let sink = factory.build(&OutputDevice::Default).await.unwrap();
            let buf = forge_audio::PcmBuffer::new(vec![0i16; 100], 44_100, 1);
            sink.play(buf).await.unwrap();
        });
    }
}
