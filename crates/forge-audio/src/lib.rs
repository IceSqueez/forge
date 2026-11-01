mod device;
mod error;
mod pcm;
mod remote;
mod sink;

pub use device::catalog::{
    DeviceId, DeviceInfo, pick_default_input_device, pick_default_output_device,
};
pub use device::device_sink::{DeviceSink, OutputDeviceHandle, stored_output_device};
pub use device::enumerate::{
    OutputDevice, list_input_devices, list_output_devices, refresh_input_devices,
    refresh_output_devices,
};
pub use device::resolve::resolve_device;
pub use device::sink_factory::{AudioSinkFactory, CpalSinkFactory, play_on_default_output};
pub use device::voice_gate::{VoiceGateConfig, VoiceGateMonitor, VoiceGateState, VoiceGateWatch};
pub use error::AudioError;
pub use pcm::alert_cue::alert_cue;
pub use pcm::decode::{decode_bytes, decode_file, probe_duration_secs};
pub use remote::messages::AUDIO_MESSAGES;
pub use remote::remote_sink::RemoteSink;
pub use remote::targeted::{PlaybackTarget, RemoteLegFactory, TargetedSinkFactory};
pub use remote::{
    ClipMediaType, PlaybackCorrelation, RemoteAudioDestination, RemoteClip, RemoteCommand,
    RemoteDelivery, RemoteDestinationId, RemoteFailure, RemoteVerdict,
};
pub use sink::fan_out::FanOutSink;
pub use sink::playback::{ControlledPlayback, PlaybackControl, PlaybackHandle};
pub use sink::route::AudioRoute;
pub use sink::{AudioSink, NullSink};
