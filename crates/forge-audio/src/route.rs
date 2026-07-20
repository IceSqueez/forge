use std::sync::Arc;

use crate::cpal_sink::CpalSink;
use crate::device::{
    DeviceId, DeviceInfo, OutputDevice, list_output_devices, pick_default_output_device,
};
use crate::error::AudioError;
use crate::events::AudioEventSink;
use crate::sink::AudioSink;

#[derive(Debug, Clone)]
pub enum DevicePreference {
    Default,
    ById(String),
    Named(String),
}

impl From<Option<String>> for DevicePreference {
    fn from(stored: Option<String>) -> Self {
        match stored {
            Some(id) => DevicePreference::ById(id),
            None => DevicePreference::Default,
        }
    }
}

impl From<&OutputDevice> for DevicePreference {
    fn from(device: &OutputDevice) -> Self {
        match device {
            OutputDevice::Default => DevicePreference::Default,
            OutputDevice::ByName { name } => DevicePreference::Named(name.clone()),
            OutputDevice::ById { id } => DevicePreference::ById(id.clone()),
        }
    }
}

impl From<OutputDevice> for DevicePreference {
    fn from(device: OutputDevice) -> Self {
        DevicePreference::from(&device)
    }
}

pub fn resolve_device(
    preference: impl Into<DevicePreference>,
    devices: &[DeviceInfo],
) -> Option<DeviceId> {
    match preference.into() {
        DevicePreference::Default => pick_default_output_device(devices),
        DevicePreference::ById(id) => devices
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
        DevicePreference::Named(name) => devices
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.id.clone())
            .or_else(|| {
                tracing::warn!(device_name = %name, "output device not found, falling back to default");
                pick_default_output_device(devices)
            }),
    }
}

pub async fn resolve_output_device(
    preference: impl Into<DevicePreference> + Send + 'static,
) -> Result<DeviceId, AudioError> {
    let preference = preference.into();
    tokio::task::spawn_blocking(move || {
        let devices = list_output_devices()?;
        resolve_device(preference, &devices).ok_or(AudioError::NoDefaultDevice)
    })
    .await
    .map_err(|e| AudioError::JoinFailed(e.to_string()))?
}

pub fn build_cpal_sink(
    device_id: DeviceId,
    event_sink: Arc<dyn AudioEventSink>,
) -> Arc<dyn AudioSink> {
    Arc::new(CpalSink::new(device_id, None, None, event_sink))
}

pub fn fan_out_targets(primary: &OutputDevice, also_headphones: bool) -> Vec<OutputDevice> {
    let mut targets = vec![primary.clone()];
    if also_headphones && !matches!(primary, OutputDevice::Default) {
        targets.push(OutputDevice::Default);
    }
    targets
}
