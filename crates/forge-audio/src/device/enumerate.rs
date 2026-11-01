use forge_types::RecoverMutex;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::device::catalog::{DeviceId, DeviceInfo, selectable_devices};
use crate::error::AudioError;

pub use forge_types::OutputDevice;

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(5);

type DeviceCache = Mutex<Option<(Instant, Vec<DeviceInfo>)>>;

static DEVICE_CACHE: DeviceCache = Mutex::new(None);
static INPUT_DEVICE_CACHE: DeviceCache = Mutex::new(None);

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

pub fn list_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    cached_devices(DeviceDirection::Input)
}

pub fn refresh_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    refreshed_devices(DeviceDirection::Input)
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
