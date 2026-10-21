use std::sync::Arc;

use forge_types::OutputDevice;

use crate::error::AudioError;
use crate::events::AudioEventSink;
use crate::pcm::PcmBuffer;
use crate::route::{build_cpal_sink, resolve_output_device};

pub async fn play_on_default_output(
    buffer: PcmBuffer,
    event_sink: Arc<dyn AudioEventSink>,
) -> Result<(), AudioError> {
    let device_id = resolve_output_device(OutputDevice::Default).await?;
    build_cpal_sink(device_id, event_sink).play(buffer).await
}
