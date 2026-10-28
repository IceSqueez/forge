#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, sound sub-action runners."]

mod audio_event;
pub mod builtin_library;
mod bus_event_sink;
mod control;
mod cpal_factory;
mod duration;
mod error;
mod fan_out;
mod hotkey_dispatch;
mod library;
mod player;
mod runners;
mod settings;
mod sink_factory;

pub use audio_event::{AudioEvent, AudioEventSink, NullAudioEventSink};
pub use builtin_library::{
    BUILTIN_FILE_EXTENSIONS, BUILTIN_SOUNDS, BuiltinSoundEntry, builtin_availability,
    resolve_builtin_path,
};
pub use bus_event_sink::BusAudioEventSink;
pub use control::SoundboardControl;
pub use cpal_factory::CpalSinkFactory;
pub use duration::probe_clip_duration_secs;
pub use error::SoundboardError;
pub use hotkey_dispatch::{clips_bound_to, pressed_combo, spawn_clip_hotkey_dispatcher};
pub use library::{
    AdoptionStep, AdoptionVerdict, ClipAvailability, ClipLibrary, ClipRefusal, ClipSource,
    SourcePlan, choose_source, clip_source_referrer, final_refusal, plan_adoption, plan_source,
    refusal_is_final,
};
pub use player::{ClipRoute, ClipToggle, PlayOutcome, SoundboardPlayer};
pub use runners::{
    PlaySoundRunner, SetMasterVolumeRunner, StopAllSoundsRunner, StopSoundRunner,
    register_soundboard_sub_actions,
};
pub use settings::{SoundboardSettings, SoundboardSettingsHandle, load_soundboard_settings};
pub use sink_factory::AudioSinkFactory;
