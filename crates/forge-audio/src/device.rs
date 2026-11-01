use forge_types::RecoverMutex;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::AudioError;

pub(crate) const CANONICAL_OUTPUT_CHAIN: &[&str] = &["default", "pipewire", "pulse"];
pub(crate) const CANONICAL_INPUT_CHAIN: &[&str] = &["default", "pipewire", "pulse"];

const NULL_DEVICE_ID: &str = "null";

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
    id_str == NULL_DEVICE_ID
        || NOISE_ID_PREFIXES
            .iter()
            .any(|prefix| id_str.starts_with(prefix))
}

pub use forge_types::OutputDevice;

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(5);

type DeviceCache = Mutex<Option<(Instant, Vec<DeviceInfo>)>>;

static DEVICE_CACHE: DeviceCache = Mutex::new(None);
static INPUT_DEVICE_CACHE: DeviceCache = Mutex::new(None);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

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

pub fn list_output_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    cached_devices(DeviceDirection::Output)
}

pub(crate) fn forget_output_devices() {
    {
        let mut guard = DeviceDirection::Output.cache().lock_recover();
        *guard = None;
    }
}

pub fn refresh_output_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    refreshed_devices(DeviceDirection::Output)
}

pub fn pick_default_output_device(devices: &[DeviceInfo]) -> Option<DeviceId> {
    pick_default_device(CANONICAL_OUTPUT_CHAIN, devices)
}

pub fn list_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    cached_devices(DeviceDirection::Input)
}

pub fn refresh_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    refreshed_devices(DeviceDirection::Input)
}

pub fn pick_default_input_device(devices: &[DeviceInfo]) -> Option<DeviceId> {
    pick_default_device(CANONICAL_INPUT_CHAIN, devices)
}

#[derive(Clone, Copy)]
enum DeviceDirection {
    Output,
    Input,
}

impl DeviceDirection {
    fn cache(self) -> &'static DeviceCache {
        match self {
            Self::Output => &DEVICE_CACHE,
            Self::Input => &INPUT_DEVICE_CACHE,
        }
    }

    fn host_devices(
        self,
        host: &cpal::Host,
    ) -> Result<(Option<cpal::Device>, Vec<cpal::Device>), AudioError> {
        use cpal::traits::HostTrait;

        let listing = match self {
            Self::Output => host
                .output_devices()
                .map(|devices| (host.default_output_device(), devices.collect())),
            Self::Input => host
                .input_devices()
                .map(|devices| (host.default_input_device(), devices.collect())),
        };
        listing.map_err(|e| AudioError::Host(e.to_string()))
    }
}

fn cached_devices(direction: DeviceDirection) -> Result<Vec<DeviceInfo>, AudioError> {
    if let Some((stamp, ref devices)) = *direction.cache().lock_recover()
        && stamp.elapsed() < DEVICE_CACHE_TTL
    {
        return Ok(devices.clone());
    }
    refreshed_devices(direction)
}

fn refreshed_devices(direction: DeviceDirection) -> Result<Vec<DeviceInfo>, AudioError> {
    let fresh = enumerate_uncached(direction)?;
    {
        let mut guard = direction.cache().lock_recover();
        *guard = Some((Instant::now(), fresh.clone()));
    }
    Ok(fresh)
}

fn enumerate_uncached(direction: DeviceDirection) -> Result<Vec<DeviceInfo>, AudioError> {
    use cpal::traits::DeviceTrait;

    let host = cpal::default_host();
    let (default_device, devices) = direction.host_devices(&host)?;
    let default_name: String = default_device
        .and_then(|d| d.description().ok())
        .map(|d| d.name().to_owned())
        .unwrap_or_default();

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
    Ok(selectable_devices(all))
}

fn selectable_devices(all: Vec<DeviceInfo>) -> Vec<DeviceInfo> {
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

    out
}

fn pick_default_device(chain: &[&str], devices: &[DeviceInfo]) -> Option<DeviceId> {
    chain
        .iter()
        .find_map(|preferred| devices.iter().find(|d| d.id.as_str() == *preferred))
        .or_else(|| {
            devices
                .iter()
                .find(|d| d.is_default && d.id.as_str() != NULL_DEVICE_ID)
        })
        .or_else(|| devices.iter().find(|d| d.id.as_str() != NULL_DEVICE_ID))
        .or_else(|| devices.first())
        .map(|d| d.id.clone())
}

#[cfg(test)]
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
    fn the_canonical_chain_wins_and_the_flagged_device_is_the_fallback() {
        for (label, devices, input) in [
            (
                "canonical_chain_beats_is_default_flagged_sysdefault",
                [dev("sysdefault", true), dev("pipewire", false)],
                "pipewire",
            ),
            (
                "falls_back_to_is_default_when_no_canonical_name_present",
                [dev("BuiltInOutput", false), dev("USB DAC", true)],
                "USB DAC",
            ),
        ] {
            assert_eq!(
                pick_default_output_device(&devices),
                Some(DeviceId::new(input)),
                "{label}"
            );
        }
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

    fn named(id: &str, name: &str, is_default: bool) -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::new(id),
            name: name.to_owned(),
            is_default,
        }
    }

    fn ids(devices: &[DeviceInfo]) -> Vec<&str> {
        devices.iter().map(|d| d.id.as_str()).collect()
    }

    #[test]
    fn selectable_devices_drop_noise_and_keep_the_first_of_each_name() {
        let listed = vec![
            named("hw:CARD=Mic", "Mic", false),
            named("pipewire", "Speakers", false),
            named("speakers-copy", "Speakers", false),
            named("headset", "Headset", false),
        ];

        let kept = selectable_devices(listed);

        assert_eq!(ids(&kept), ["pipewire", "headset"]);
    }

    #[test]
    fn a_default_hidden_as_a_duplicate_name_is_listed_again() {
        let listed = vec![
            named("speakers-a", "Speakers", false),
            named("speakers-b", "Speakers", true),
        ];

        let kept = selectable_devices(listed);

        assert_eq!(ids(&kept), ["speakers-a", "speakers-b"]);
    }

    #[test]
    fn a_noise_default_is_never_listed_again() {
        let listed = vec![
            named("hw:CARD=Mic", "Mic", true),
            named("pipewire", "Speakers", false),
        ];

        let kept = selectable_devices(listed);

        assert_eq!(ids(&kept), ["pipewire"]);
    }

    #[test]
    fn a_kept_default_is_not_listed_twice() {
        let kept = selectable_devices(vec![named("pipewire", "Speakers", true)]);

        assert_eq!(ids(&kept), ["pipewire"]);
    }
}
