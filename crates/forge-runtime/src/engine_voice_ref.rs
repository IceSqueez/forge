use std::fmt;

use crate::speak_dispatcher::VoiceDescriptor;

pub const ENGINE_VOICE_OPTIONS_KEY: &str = "tts.engine_voices";

const ENGINE_VOICE_SEPARATOR: char = '/';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineVoiceRef {
    pub engine_id: String,
    pub voice_id: String,
}

impl EngineVoiceRef {
    pub fn new(engine_id: impl Into<String>, voice_id: impl Into<String>) -> Self {
        Self {
            engine_id: engine_id.into(),
            voice_id: voice_id.into(),
        }
    }

    pub fn of(voice: &VoiceDescriptor) -> Self {
        Self::new(voice.engine_id.clone(), voice.id.clone())
    }

    pub fn parse(encoded: &str) -> Option<Self> {
        let (engine_id, voice_id) = encoded.trim().split_once(ENGINE_VOICE_SEPARATOR)?;
        if engine_id.is_empty() || voice_id.is_empty() {
            return None;
        }
        Some(Self::new(engine_id, voice_id))
    }

    pub fn is_installed_in(&self, voices: &[VoiceDescriptor]) -> bool {
        voices
            .iter()
            .any(|voice| voice.engine_id == self.engine_id && voice.id == self.voice_id)
    }
}

impl fmt::Display for EngineVoiceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{ENGINE_VOICE_SEPARATOR}{}",
            self.engine_id, self.voice_id
        )
    }
}
