use forge_types::OutputDevice;

use crate::error::AudioError;
use crate::route::{build_cpal_sink, resolve_output_device};
use forge_speech_types::PcmBuffer;

pub async fn play_on_default_output(buffer: PcmBuffer) -> Result<(), AudioError> {
    let device_id = resolve_output_device(OutputDevice::Default).await?;
    build_cpal_sink(device_id).play(buffer).await
}
