use std::sync::Arc;

use async_trait::async_trait;
use forge_types::{OutputDevice, Shared};

use crate::cpal_sink::CpalSink;
use crate::error::AudioError;
use crate::events::AudioEventSink;
use crate::handle::{ControlledPlayback, PlaybackHandle};
use crate::pcm::PcmBuffer;
use crate::route::resolve_output_device;
use crate::sink::AudioSink;

pub fn stored_output_device(stored_id: Option<String>) -> OutputDevice {
    match stored_id {
        Some(id) => OutputDevice::ById { id },
        None => OutputDevice::Default,
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
    event_sink: Arc<dyn AudioEventSink>,
}

impl DeviceSink {
    pub fn new(device: OutputDeviceHandle, event_sink: Arc<dyn AudioEventSink>) -> Self {
        Self { device, event_sink }
    }

    pub fn device_handle(&self) -> OutputDeviceHandle {
        self.device.clone()
    }

    async fn current(&self) -> Result<CpalSink, AudioError> {
        let device = self.device.load().as_ref().clone();
        let device_id = resolve_output_device(device).await?;
        Ok(CpalSink::new(
            device_id,
            None,
            None,
            Arc::clone(&self.event_sink),
        ))
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
