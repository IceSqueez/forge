use crate::device::catalog::{DeviceId, DeviceInfo, pick_default_output_device};
use crate::device::enumerate::{OutputDevice, list_output_devices};
use crate::error::AudioError;

pub fn resolve_device(preference: &OutputDevice, devices: &[DeviceInfo]) -> Option<DeviceId> {
    match preference {
        OutputDevice::Default => pick_default_output_device(devices),
        OutputDevice::ById { id } => devices
            .iter()
            .find(|d| d.id.as_str() == id)
            .map(|d| d.id.clone())
            .or_else(|| {
                tracing::warn!(
                    device_id = %id,
                    "stored output device id not found among enumerated devices, falling back to default"
                );
                pick_default_output_device(devices)
            }),
        OutputDevice::ByName { name } => devices
            .iter()
            .find(|d| d.name == *name)
            .map(|d| d.id.clone())
            .or_else(|| {
                tracing::warn!(device_name = %name, "output device not found, falling back to default");
                pick_default_output_device(devices)
            }),
    }
}

pub(crate) async fn resolve_output_device(
    preference: OutputDevice,
) -> Result<DeviceId, AudioError> {
    tokio::task::spawn_blocking(move || {
        let devices = list_output_devices()?;
        resolve_device(&preference, &devices).ok_or(AudioError::NoDefaultDevice)
    })
    .await
    .map_err(|e| AudioError::JoinFailed(e.to_string()))?
}
