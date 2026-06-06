use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::AudioError;

pub use forge_types::OutputDevice;

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(5);

static DEVICE_CACHE: Mutex<Option<(Instant, Vec<DeviceInfo>)>> = Mutex::new(None);

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
/// with no audio support. Results cached for 5s — cpal enumeration is expensive
/// on Linux (PipeWire round-trip) and gets called repeatedly by the audio device
/// picker subscription.
pub fn list_output_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    if let Ok(guard) = DEVICE_CACHE.lock()
        && let Some((stamp, ref devices)) = *guard
        && stamp.elapsed() < DEVICE_CACHE_TTL
    {
        return Ok(devices.clone());
    }
    let fresh = enumerate_uncached()?;
    if let Ok(mut guard) = DEVICE_CACHE.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }
    Ok(fresh)
}

/// Bypass the 5s cache. Used by the "Refresh devices" button so the user always
/// sees the live device list after plugging/unplugging hardware.
pub fn refresh_output_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let fresh = enumerate_uncached()?;
    if let Ok(mut guard) = DEVICE_CACHE.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }
    Ok(fresh)
}

fn enumerate_uncached() -> Result<Vec<DeviceInfo>, AudioError> {
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
        let id_str = device
            .id()
            .map(|id| id.id().to_owned())
            .unwrap_or_else(|_| name.clone());
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
}
