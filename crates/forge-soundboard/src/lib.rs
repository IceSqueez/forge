#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, sound sub-action runners."]

mod audio_event;
mod builtin_library;
mod bus_event_sink;
mod control;
mod duration;
mod error;
mod fan_out;
mod hotkey_dispatch;
mod library;
mod player;
mod runners;
pub mod settings;
pub mod vocabulary;

pub use audio_event::AudioEvent;
pub use audio_event::AudioEventSink;
pub use audio_event::NullAudioEventSink;
pub use builtin_library::{
    BUILTIN_SOUNDS, BuiltinSoundEntry, builtin_availability, resolve_builtin_path,
};
pub use bus_event_sink::BusAudioEventSink;
pub use control::SoundboardControl;
pub use error::SoundboardError;
pub use hotkey_dispatch::{pressed_combo, spawn_clip_hotkey_dispatcher};
pub use library::{AdoptionVerdict, ClipAvailability, ClipLibrary, ClipRefusal, ClipSource};
pub use player::ClipToggle;
pub use player::{ClipRoute, PlayOutcome, SoundboardPlayer};
pub use runners::{
    PlaySoundRunner, SetMasterVolumeRunner, StopAllSoundsRunner, StopSoundRunner,
    register_soundboard_sub_actions,
};
pub use settings::{SoundboardSettings, SoundboardSettingsHandle, load_soundboard_settings};
