#![doc = "Audio output abstraction: AudioSink trait, cpal device discovery, multi-sink fan-out, format conversion."]

pub mod device;
pub mod error;
pub mod pcm;
pub mod sink;

pub use device::{DeviceId, DeviceInfo, OutputDevice, list_output_devices};
pub use error::AudioError;
pub use pcm::PcmBuffer;
pub use sink::{AudioSink, NullSink};
