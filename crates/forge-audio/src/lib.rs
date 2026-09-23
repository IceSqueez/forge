#![doc = "Audio output abstraction: AudioSink trait, cpal device discovery, multi-sink fan-out, format conversion."]

pub mod audio_route;
pub mod convert;
pub mod cpal_sink;
pub mod decode;
pub mod device;
pub mod device_sink;
pub mod error;
pub mod events;
pub mod fan_out;
pub mod handle;
pub mod pcm;
pub mod remote;
pub mod route;
pub mod sink;
pub mod sink_factory;
pub mod voice_gate;

pub use audio_route::AudioRoute;
pub use cpal_sink::CpalSink;
pub use decode::{decode_bytes, decode_file, probe_duration_secs};
pub use device::{
    DeviceId, DeviceInfo, OutputDevice, list_input_devices, list_output_devices,
    pick_default_input_device, pick_default_output_device, refresh_input_devices,
    refresh_output_devices,
};
pub use device_sink::{DeviceSink, OutputDeviceHandle, stored_output_device};
pub use error::AudioError;
pub use events::{AudioEvent, AudioEventSink, NullAudioEventSink};
pub use fan_out::{FanOutSink, fan_out_stoppable};
pub use handle::{ControlledPlayback, PlaybackHandle};
pub use pcm::PcmBuffer;
pub use remote::{
    ClipMediaType, RemoteAudioDestination, RemoteClip, RemoteClipId, RemoteCommand, RemoteDelivery,
    RemoteDestinationId, RemoteVerdict,
};
pub use route::{
    DevicePreference, build_cpal_sink, fan_out_targets, resolve_device, resolve_output_device,
};
pub use sink::{AudioSink, NullSink};
pub use sink_factory::{AudioSinkFactory, CpalSinkFactory};
pub use voice_gate::{VoiceGateConfig, VoiceGateMonitor, VoiceGateState};
