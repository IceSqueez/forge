use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::AudioError;

pub const CANONICAL_OUTPUT_CHAIN: &[&str] = &["default", "pipewire", "pulse"];
pub const CANONICAL_INPUT_CHAIN: &[&str] = &["default", "pipewire", "pulse"];

const NOISE_ID_PREFIXES: &[&str] = &[
    "sysdefault",
    "dmix",
    "dsnoop",
    "dcmix",
    "surround",
    "front",
    "rear",
    "center_lfe",
    "side",
    "iec958",
    "spdif",
    "hdmi",
    "usbstream",
    "samplerate",
    "speexrate",
    "upmix",
    "vdownmix",
    "lavrate",
    "plug:",
    "plughw:",
    "hw:",
];

fn is_noise_device_id(id_str: &str) -> bool {
    id_str == "null"
        || NOISE_ID_PREFIXES
            .iter()
            .any(|prefix| id_str.starts_with(prefix))
}

pub use forge_types::OutputDevice;

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(5);

static DEVICE_CACHE: Mutex<Option<(Instant, Vec<DeviceInfo>)>> = Mutex::new(None);
static INPUT_DEVICE_CACHE: Mutex<Option<(Instant, Vec<DeviceInfo>)>> = Mutex::new(None);

/// Backend-defined string under the hood; callers must not parse it.
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

/// Cached for 5s: cpal enumeration is expensive on Linux (PipeWire round-trip).
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

/// Bypasses the 5s cache so a device picker refresh sees just-plugged hardware.
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

    let mut all = Vec::new();
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
        all.push(DeviceInfo {
            id: DeviceId::new(id_str),
            name,
            is_default,
        });
    }

    let default_entry = all.iter().find(|d| d.is_default).cloned();

    let mut out = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    for device in all {
        if is_noise_device_id(device.id.as_str()) {
            continue;
        }
        if !seen_names.insert(device.name.clone()) {
            continue;
        }
        out.push(device);
    }

    if !out.iter().any(|d| d.is_default)
        && let Some(default_entry) = default_entry
        && !is_noise_device_id(default_entry.id.as_str())
    {
        out.push(default_entry);
    }

    Ok(out)
}

pub fn pick_default_output_device(devices: &[DeviceInfo]) -> Option<DeviceId> {
    CANONICAL_OUTPUT_CHAIN
        .iter()
        .find_map(|preferred| devices.iter().find(|d| d.id.as_str() == *preferred))
        .or_else(|| {
            devices
                .iter()
                .find(|d| d.is_default && d.id.as_str() != "null")
        })
        .or_else(|| devices.iter().find(|d| d.id.as_str() != "null"))
        .or_else(|| devices.first())
        .map(|d| d.id.clone())
}

/// Cached for 5s: cpal enumeration is expensive on Linux (PipeWire round-trip).
pub fn list_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    if let Ok(guard) = INPUT_DEVICE_CACHE.lock()
        && let Some((stamp, ref devices)) = *guard
        && stamp.elapsed() < DEVICE_CACHE_TTL
    {
        return Ok(devices.clone());
    }
    let fresh = enumerate_input_uncached()?;
    if let Ok(mut guard) = INPUT_DEVICE_CACHE.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }
    Ok(fresh)
}

/// Bypasses the 5s cache so a device picker refresh sees just-plugged hardware.
pub fn refresh_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let fresh = enumerate_input_uncached()?;
    if let Ok(mut guard) = INPUT_DEVICE_CACHE.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }
    Ok(fresh)
}

fn enumerate_input_uncached() -> Result<Vec<DeviceInfo>, AudioError> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let default_name: String = host
        .default_input_device()
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_owned())
        .unwrap_or_default();

    let devices = host
        .input_devices()
        .map_err(|e| AudioError::Host(e.to_string()))?;

    let mut all = Vec::new();
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
        all.push(DeviceInfo {
            id: DeviceId::new(id_str),
            name,
            is_default,
        });
    }

    let default_entry = all.iter().find(|d| d.is_default).cloned();

    let mut out = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    for device in all {
        if is_noise_device_id(device.id.as_str()) {
            continue;
        }
        if !seen_names.insert(device.name.clone()) {
            continue;
        }
        out.push(device);
    }

    if !out.iter().any(|d| d.is_default)
        && let Some(default_entry) = default_entry
        && !is_noise_device_id(default_entry.id.as_str())
    {
        out.push(default_entry);
    }

    Ok(out)
}

pub fn pick_default_input_device(devices: &[DeviceInfo]) -> Option<DeviceId> {
    CANONICAL_INPUT_CHAIN
        .iter()
        .find_map(|preferred| devices.iter().find(|d| d.id.as_str() == *preferred))
        .or_else(|| {
            devices
                .iter()
                .find(|d| d.is_default && d.id.as_str() != "null")
        })
        .or_else(|| devices.iter().find(|d| d.id.as_str() != "null"))
        .or_else(|| devices.first())
        .map(|d| d.id.clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn dev(id: &str, is_default: bool) -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::new(id),
            name: id.to_owned(),
            is_default,
        }
    }

    #[test]
    fn canonical_chain_beats_is_default_flagged_sysdefault() {
        let devices = [dev("sysdefault", true), dev("pipewire", false)];
        assert_eq!(
            pick_default_output_device(&devices),
            Some(DeviceId::new("pipewire")),
        );
    }

    #[test]
    fn canonical_chain_is_consulted_in_priority_order() {
        for (devices, expected) in [
            (
                vec![
                    dev("default", false),
                    dev("pipewire", false),
                    dev("pulse", true),
                ],
                "default",
            ),
            (vec![dev("pipewire", false), dev("pulse", true)], "pipewire"),
            (
                vec![
                    dev("pulse", false),
                    dev("pipewire", false),
                    dev("default", false),
                ],
                "default",
            ),
        ] {
            assert_eq!(
                pick_default_output_device(&devices),
                Some(DeviceId::new(expected)),
                "chain order violated for {devices:?}",
            );
        }
    }

    #[test]
    fn falls_back_to_is_default_when_no_canonical_name_present() {
        let devices = [dev("BuiltInOutput", false), dev("USB DAC", true)];
        assert_eq!(
            pick_default_output_device(&devices),
            Some(DeviceId::new("USB DAC")),
        );
    }

    #[test]
    fn sysdefault_ids_are_classified_as_noise() {
        for id in ["sysdefault", "sysdefault:CARD=PCH"] {
            assert!(
                is_noise_device_id(id),
                "expected noise classification: {id}"
            );
        }
    }

    #[test]
    fn canonical_chain_ids_survive_noise_filtering() {
        for id in CANONICAL_OUTPUT_CHAIN.iter().chain(CANONICAL_INPUT_CHAIN) {
            assert!(
                !is_noise_device_id(id),
                "chain member wrongly filtered as noise: {id}"
            );
        }
    }

    #[test]
    fn input_picker_walks_the_canonical_then_default_then_non_null_ladder() {
        for (devices, expected) in [
            (
                vec![dev("sysdefault", true), dev("pipewire", false)],
                "pipewire",
            ),
            (
                vec![
                    dev("pulse", false),
                    dev("pipewire", false),
                    dev("default", true),
                ],
                "default",
            ),
            (vec![dev("pulse", true), dev("pipewire", false)], "pipewire"),
            (
                vec![dev("Built-in Mic", false), dev("USB Mic", true)],
                "USB Mic",
            ),
            (
                vec![dev("null", true), dev("Webcam Mic", false)],
                "Webcam Mic",
            ),
            (vec![dev("null", false)], "null"),
        ] {
            assert_eq!(
                pick_default_input_device(&devices),
                Some(DeviceId::new(expected)),
                "input ladder violated for {devices:?}",
            );
        }
    }

    #[test]
    fn input_picker_returns_none_when_the_host_reports_no_devices() {
        assert_eq!(pick_default_input_device(&[]), None);
    }
}
