use serde::{Deserialize, Serialize};

use crate::voice::{EngineId, TtsVoice, VoiceId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AliasId(pub String);

impl AliasId {
    pub fn new() -> Self {
        Self(ulid::Ulid::generate().to_string())
    }
}

impl Default for AliasId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AliasState {
    Active,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAlias {
    pub id: AliasId,
    pub viewer_id: String,
    pub viewer_name: String,
    pub engine_id: EngineId,
    pub voice_id: VoiceId,
    pub pitch_semitones: Option<f32>,
    pub rate_multiplier: Option<f32>,
    pub state: AliasState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AssignmentStrategy {
    #[default]
    DeterministicByName,
    Random,
    Single {
        voice_id: VoiceId,
        engine_id: EngineId,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IgnoreProfile {
    pub excluded_voice_ids: Vec<VoiceId>,
    pub excluded_locales: Vec<String>,
}

impl IgnoreProfile {
    pub fn is_eligible(&self, voice: &TtsVoice) -> bool {
        !self.excluded_voice_ids.contains(&voice.id)
            && !self.excluded_locales.contains(&voice.locale)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SynthesisDefaults {
    pub pitch_semitones: f32,
    pub rate_multiplier: f32,
}

impl Default for SynthesisDefaults {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
        }
    }
}
