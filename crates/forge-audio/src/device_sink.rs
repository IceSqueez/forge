use std::sync::Arc;

use async_trait::async_trait;
use forge_types::{OutputDevice, Shared};

use crate::error::AudioError;
use crate::events::AudioEventSink;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::route::DevicePreference;
use crate::sink::AudioSink;
use crate::sink_factory::{AudioSinkFactory, CpalSinkFactory};

pub fn stored_output_device(stored_id: Option<String>) -> OutputDevice {
    match DevicePreference::from(stored_id) {
        DevicePreference::ById(id) => OutputDevice::ById { id },
        _ => OutputDevice::Default,
    }
}

#[derive(Clone)]
pub struct OutputDeviceHandle(Shared<OutputDevice>);

impl OutputDeviceHandle {
    pub fn new(device: OutputDevice) -> Self {
        Self(Shared::new(device))
    }

    fn load(&self) -> Arc<OutputDevice> {
        self.0.load()
    }

    pub fn swap(&self, device: OutputDevice) {
        self.0.store(device);
    }
}

/// Reads the handle once per playback call, so a swap reaches the next clip and leaves a running one untouched.
pub struct DeviceSink {
    device: OutputDeviceHandle,
    factory: Arc<dyn AudioSinkFactory>,
}

impl DeviceSink {
    pub fn new(device: OutputDeviceHandle, event_sink: Arc<dyn AudioEventSink>) -> Self {
        Self::with_factory(device, Arc::new(CpalSinkFactory::new(event_sink)))
    }

    pub fn with_factory(device: OutputDeviceHandle, factory: Arc<dyn AudioSinkFactory>) -> Self {
        Self { device, factory }
    }

    pub fn device_handle(&self) -> OutputDeviceHandle {
        self.device.clone()
    }

    async fn current(&self) -> Result<Arc<dyn AudioSink>, AudioError> {
        let device = self.device.load().as_ref().clone();
        self.factory.build(&device).await
    }
}

#[async_trait]
impl AudioSink for DeviceSink {
    async fn play(&self, buffer: PcmBuffer) -> Result<(), AudioError> {
        self.current().await?.play(buffer).await
    }

    async fn play_stoppable(&self, buffer: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        self.current().await?.play_stoppable(buffer).await
    }

    async fn play_controlled(&self, buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.current().await?.play_controlled(buffer).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    use crate::device::{DeviceId, DeviceInfo};
    use crate::events::NullAudioEventSink;
    use crate::route::resolve_device;

    const DEFAULT_ID: &str = "default";
    const HEADSET_ID: &str = "hw:1";
    const DECOY_ID: &str = "hw:2";
    const RETIRED_ID: &str = "hw:9";

    fn enumerated_devices() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo {
                id: DeviceId::new(DEFAULT_ID),
                name: "Built-in Audio".to_owned(),
                is_default: true,
            },
            DeviceInfo {
                id: DeviceId::new(HEADSET_ID),
                name: "USB Headset".to_owned(),
                is_default: false,
            },
            DeviceInfo {
                id: DeviceId::new(DECOY_ID),
                name: HEADSET_ID.to_owned(),
                is_default: false,
            },
        ]
    }

    #[test]
    fn a_stored_device_resolves_by_id_and_falls_back_to_the_default_when_it_is_gone() {
        for (case, stored, expected) in [
            (
                "nothing stored plays on the system default",
                None,
                DEFAULT_ID,
            ),
            (
                "a stored id outranks a device merely named after it",
                Some(HEADSET_ID),
                HEADSET_ID,
            ),
            (
                "a stored device that is no longer enumerated falls back",
                Some(RETIRED_ID),
                DEFAULT_ID,
            ),
        ] {
            let resolved = resolve_device(
                stored_output_device(stored.map(str::to_owned)),
                &enumerated_devices(),
            );
            assert_eq!(
                resolved.as_ref().map(DeviceId::as_str),
                Some(expected),
                "{case}"
            );
        }
    }

    #[test]
    fn the_handle_a_sink_hands_out_reroutes_that_same_sink() {
        let sink = DeviceSink::new(
            OutputDeviceHandle::new(OutputDevice::Default),
            Arc::new(NullAudioEventSink),
        );

        sink.device_handle().swap(OutputDevice::ById {
            id: HEADSET_ID.to_owned(),
        });

        assert_eq!(
            sink.device.load().as_ref(),
            &OutputDevice::ById {
                id: HEADSET_ID.to_owned()
            },
            "the settings screen must reroute the live sink, not a detached copy of its device"
        );
    }
}
