#![doc = "Audio output abstraction: AudioSink trait, cpal device discovery, multi-sink fan-out, format conversion."]

pub mod convert;
pub mod cpal_sink;
pub mod decode;
pub mod device;
pub mod error;
pub mod events;
pub mod fan_out;
pub mod handle;
pub mod pcm;
pub mod route;
pub mod sink;
pub mod voice_gate;

pub use cpal_sink::CpalSink;
pub use decode::{decode_bytes, decode_file, probe_duration_secs};
pub use device::{
    DeviceId, DeviceInfo, OutputDevice, list_input_devices, list_output_devices,
    pick_default_input_device, pick_default_output_device, refresh_input_devices,
    refresh_output_devices,
};
pub use error::AudioError;
pub use events::{AudioEvent, AudioEventSink, NullAudioEventSink};
pub use fan_out::fan_out_stoppable;
pub use handle::{ControlledPlayback, PlaybackHandle};
pub use pcm::PcmBuffer;
pub use route::{
    DevicePreference, build_cpal_sink, fan_out_targets, resolve_device, resolve_output_device,
};
pub use sink::{AudioSink, NullSink};
pub use voice_gate::{VoiceGateConfig, VoiceGateMonitor, VoiceGateState};
