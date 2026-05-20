#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, PlaySound runner."]

pub mod bus_event_sink;
pub mod clip;
pub mod cpal_factory;
pub mod error;
pub mod player;
pub mod sink_factory;

pub use bus_event_sink::BusAudioEventSink;
pub use clip::{ClipVolume, SoundboardClip};
pub use cpal_factory::CpalSinkFactory;
pub use error::SoundboardError;
pub use player::SoundboardPlayer;
pub use sink_factory::AudioSinkFactory;
