use serde::{Deserialize, Serialize};

use crate::error::AudioError;

pub use forge_types::OutputDevice;

/// Opaque stable handle for an output device. Backend-defined string under the hood —
/// callers must not parse it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub is_default: bool,
}

/// Enumerate output devices via the default cpal host. Returns an empty Vec on hosts
/// with no audio support (CI builds without an audio runtime).
pub fn list_output_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let default_name: String = host
        .default_output_device()
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_owned())
        .unwrap_or_default();

    let devices = host
        .output_devices()
        .map_err(|e| AudioError::Host(e.to_string()))?;

    let mut out = Vec::new();
    for device in devices {
        let Ok(desc) = device.description() else {
            continue;
        };
        let name = desc.name().to_owned();
        let id_str = device.id().map(|id| id.1).unwrap_or_else(|_| name.clone());
        let is_default = !default_name.is_empty() && name == default_name;
        out.push(DeviceInfo {
            id: DeviceId::new(id_str),
            name,
            is_default,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn device_id_round_trip_as_str() {
        let id = DeviceId::new("hw:0,0");
        assert_eq!(id.as_str(), "hw:0,0");
    }

    #[test]
    fn device_id_serde_roundtrip() {
        let id = DeviceId::new("alsa-default");
        let json = serde_json::to_string(&id).unwrap();
        let back: DeviceId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    #[ignore = "requires audio host (cpal) — run manually with `cargo test -- --ignored`"]
    fn list_output_devices_runs_on_audio_host() {
        let result = list_output_devices();
        assert!(result.is_ok(), "list failed: {:?}", result.err());
    }
}
