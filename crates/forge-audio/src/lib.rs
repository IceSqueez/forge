#![doc = "Audio output abstraction: AudioSink trait, cpal device discovery, multi-sink fan-out, format conversion."]

pub mod alert_cue;
mod audio_route;
pub mod convert;
mod cpal_sink;
mod decode;
mod default_output;
mod device;
mod device_sink;
mod error;
mod events;
mod fan_out;
mod handle;
mod pcm;
mod remote;
mod remote_sink;
mod route;
mod sink;
mod sink_factory;
mod targeted;
mod voice_gate;
mod wave;

pub use alert_cue::alert_cue;
pub use audio_route::AudioRoute;
pub use cpal_sink::CpalSink;
pub use decode::{decode_bytes, decode_file, probe_duration_secs};
pub use default_output::play_on_default_output;
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
    ClipMediaType, PlaybackCorrelation, RemoteAudioDestination, RemoteClip, RemoteClipId,
    RemoteCommand, RemoteDelivery, RemoteDestinationId, RemoteVerdict,
};
pub use remote_sink::RemoteSink;
pub use route::{
    DevicePreference, build_cpal_sink, fan_out_targets, resolve_device, resolve_output_device,
};
pub use sink::{AudioSink, NullSink};
pub use sink_factory::{AudioSinkFactory, CpalSinkFactory};
pub use targeted::{PlaybackTarget, RemoteLegFactory, TargetedSinkFactory};
pub use voice_gate::{VoiceGateConfig, VoiceGateMonitor, VoiceGateState};
pub use wave::encode_wave;
