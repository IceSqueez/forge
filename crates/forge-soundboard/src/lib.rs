#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, PlaySound runner."]

pub mod builtin_library;
pub mod bus_event_sink;
pub mod clip;
pub mod cpal_factory;
pub mod duration;
pub mod error;
pub mod player;
pub mod settings;
pub mod sink_factory;

pub use builtin_library::{
    BUILTIN_FILE_EXTENSIONS, BUILTIN_SOUNDS, BuiltinSoundEntry, builtin_availability,
    resolve_builtin_path,
};
pub use bus_event_sink::BusAudioEventSink;
pub use clip::{ClipVolume, SoundboardClip};
pub use cpal_factory::CpalSinkFactory;
pub use duration::probe_clip_duration_secs;
pub use error::SoundboardError;
pub use player::SoundboardPlayer;
pub use settings::{SoundboardSettings, SoundboardSettingsHandle, load_soundboard_settings};
pub use sink_factory::AudioSinkFactory;
