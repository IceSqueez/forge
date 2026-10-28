use serde::{Deserialize, Serialize};

pub type Locale = String;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VoiceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceGender {
    Male,
    Female,
    Neutral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoice {
    pub id: VoiceId,
    pub name: String,
    pub locale: Locale,
    pub gender: VoiceGender,
    pub engine_id: EngineId,
    pub is_neural: bool,
    pub sample_rate_hint: u32,
}
