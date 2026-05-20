#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, PlaySound runner."]

pub mod clip;
pub mod cpal_factory;
pub mod error;
pub mod sink_factory;

pub use clip::{ClipVolume, SoundboardClip};
pub use cpal_factory::CpalSinkFactory;
pub use error::SoundboardError;
pub use sink_factory::AudioSinkFactory;
