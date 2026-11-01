mod alias;
mod language;
mod pcm;
mod voice;

pub use alias::{
    AliasId, AliasState, AssignmentStrategy, IgnoreProfile, SynthesisDefaults, VoiceAlias,
};
pub use language::LanguageCode;
pub use pcm::PcmBuffer;
pub use voice::{EngineId, Locale, TtsVoice, VoiceGender, VoiceId};

pub const ENGINE_VOICE_OPTIONS_KEY: &str = "tts.engine_voices";
