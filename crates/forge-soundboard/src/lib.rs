#![doc = "Soundboard: clip storage, playback, in-app hotkey wiring, PlaySound runner."]

pub mod builtin_library;
pub mod bus_event_sink;
pub mod cpal_factory;
pub mod duration;
pub mod error;
pub mod library;
pub mod player;
pub mod settings;
pub mod sink_factory;

pub use builtin_library::{
    BUILTIN_FILE_EXTENSIONS, BUILTIN_SOUNDS, BuiltinSoundEntry, builtin_availability,
    resolve_builtin_path,
};
pub use bus_event_sink::BusAudioEventSink;
pub use cpal_factory::CpalSinkFactory;
pub use duration::probe_clip_duration_secs;
pub use error::SoundboardError;
pub use library::{
    AdoptionStep, AdoptionVerdict, ClipAvailability, ClipLibrary, ClipRefusal, ClipSource,
    SourcePlan, choose_source, clip_source_referrer, final_refusal, plan_adoption, plan_source,
    refusal_is_final,
};
pub use player::{ClipRoute, SoundboardPlayer};
pub use settings::{SoundboardSettings, SoundboardSettingsHandle, load_soundboard_settings};
pub use sink_factory::AudioSinkFactory;
