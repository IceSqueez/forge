#![doc = "Audio output abstraction: AudioSink trait, cpal device discovery, multi-sink fan-out, format conversion."]

pub mod convert;
pub mod cpal_sink;
pub mod decode;
pub mod device;
pub mod error;
pub mod events;
pub mod fan_out;
pub mod pcm;
pub mod sink;

pub use cpal_sink::CpalSink;
pub use decode::decode_file;
pub use device::{DeviceId, DeviceInfo, OutputDevice, list_output_devices};
pub use error::AudioError;
pub use events::{AudioEvent, AudioEventSink, NullAudioEventSink};
pub use pcm::PcmBuffer;
pub use sink::{AudioSink, NullSink};
