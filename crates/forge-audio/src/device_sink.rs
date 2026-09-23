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

    use std::sync::Mutex;

    use tokio::sync::Notify;

    use crate::device::{DeviceId, DeviceInfo};
    use crate::route::resolve_device;

    const DEFAULT_ID: &str = "default";
    const HEADSET_ID: &str = "hw:1";
    const DECOY_ID: &str = "hw:2";
    const RETIRED_ID: &str = "hw:9";

    const DEVICE_REFUSAL: &str = "output device is busy";
    const SAMPLE_RATE: u32 = 22_050;
    const MONO: u16 = 1;

    fn clip() -> PcmBuffer {
        PcmBuffer::new(vec![0i16; 8], SAMPLE_RATE, MONO)
    }

    fn headset() -> OutputDevice {
        OutputDevice::ById {
            id: HEADSET_ID.to_owned(),
        }
    }

    struct RecordingFactory {
        built: Mutex<Vec<OutputDevice>>,
        finished: Arc<Mutex<Vec<OutputDevice>>>,
        gate: Option<Arc<Notify>>,
    }

    impl RecordingFactory {
        fn new(gate: Option<Arc<Notify>>) -> Arc<Self> {
            Arc::new(Self {
                built: Mutex::new(Vec::new()),
                finished: Arc::new(Mutex::new(Vec::new())),
                gate,
            })
        }

        fn built(&self) -> Vec<OutputDevice> {
            self.built.lock().unwrap().clone()
        }

        fn finished(&self) -> Vec<OutputDevice> {
            self.finished.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl AudioSinkFactory for RecordingFactory {
        async fn build(&self, device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            self.built.lock().unwrap().push(device.clone());
            Ok(Arc::new(RecordingSink {
                device: device.clone(),
                finished: Arc::clone(&self.finished),
                gate: self.gate.clone(),
            }))
        }
    }

    struct RecordingSink {
        device: OutputDevice,
        finished: Arc<Mutex<Vec<OutputDevice>>>,
        gate: Option<Arc<Notify>>,
    }

    #[async_trait]
    impl AudioSink for RecordingSink {
        async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
            Ok(())
        }

        async fn play_controlled(
            &self,
            _buffer: PcmBuffer,
        ) -> Result<ControlledPlayback, AudioError> {
            let device = self.device.clone();
            let finished = Arc::clone(&self.finished);
            let gate = self.gate.clone();
            Ok(ControlledPlayback::from_future(async move {
                if let Some(gate) = gate {
                    gate.notified().await;
                }
                finished.lock().unwrap().push(device);
                Ok(())
            }))
        }
    }

    struct RefusingFactory;

    #[async_trait]
    impl AudioSinkFactory for RefusingFactory {
        async fn build(&self, _device: &OutputDevice) -> Result<Arc<dyn AudioSink>, AudioError> {
            Err(AudioError::Host(DEVICE_REFUSAL.to_owned()))
        }
    }

    #[tokio::test]
    async fn the_factory_is_asked_on_every_play_so_a_swap_reaches_the_next_clip() {
        let factory = RecordingFactory::new(None);
        let sink = DeviceSink::with_factory(
            OutputDeviceHandle::new(OutputDevice::Default),
            Arc::clone(&factory) as Arc<dyn AudioSinkFactory>,
        );

        sink.play(clip()).await.unwrap();
        sink.play(clip()).await.unwrap();
        sink.device_handle().swap(headset());
        sink.play(clip()).await.unwrap();

        assert_eq!(
            factory.built(),
            vec![OutputDevice::Default, OutputDevice::Default, headset()]
        );
    }

    #[tokio::test]
    async fn a_swap_mid_clip_leaves_the_running_one_on_its_own_device_and_takes_the_next() {
        let gate = Arc::new(Notify::new());
        let factory = RecordingFactory::new(Some(Arc::clone(&gate)));
        let sink = DeviceSink::with_factory(
            OutputDeviceHandle::new(OutputDevice::Default),
            Arc::clone(&factory) as Arc<dyn AudioSinkFactory>,
        );

        let running = sink.play_controlled(clip()).await.unwrap();
        sink.device_handle().swap(headset());
        gate.notify_one();
        running.await.unwrap();

        assert_eq!(
            factory.finished(),
            vec![OutputDevice::Default],
            "a swap mid-clip must not move the clip that is already playing"
        );

        let next = sink.play_controlled(clip()).await.unwrap();
        gate.notify_one();
        next.await.unwrap();

        assert_eq!(
            factory.finished(),
            vec![OutputDevice::Default, headset()],
            "the clip after the swap must play on the newly chosen device"
        );
    }

    #[tokio::test]
    async fn a_device_the_factory_cannot_open_reaches_the_caller_as_a_typed_error() {
        let sink = DeviceSink::with_factory(
            OutputDeviceHandle::new(OutputDevice::Default),
            Arc::new(RefusingFactory),
        );

        let refused = |outcome: Result<(), AudioError>, entry_point: &str| {
            assert!(
                matches!(&outcome, Err(AudioError::Host(reason)) if reason == DEVICE_REFUSAL),
                "{entry_point} swallowed the refusal, got {outcome:?}"
            );
        };

        refused(sink.play(clip()).await, "play");
        refused(
            sink.play_stoppable(clip()).await.map(|_| ()),
            "play_stoppable",
        );
        refused(
            sink.play_controlled(clip()).await.map(|_| ()),
            "play_controlled",
        );
    }

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
}
