#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, PlaySound runner."]

pub mod clip;
pub mod error;

pub use clip::{ClipVolume, SoundboardClip};
pub use error::SoundboardError;
