use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use forge_audio::PcmBuffer;

/// BCP-47 locale string, e.g. `"uk-UA"`, `"en-US"`.
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

/// Voice metadata returned by `TtsEngine::list_voices`.
///
/// `sample_rate_hint` is the engine's preferred output rate in Hz; callers
/// must not assume the actual PCM rate matches — use `PcmBuffer::sample_rate`.
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

/// Input to a single synthesis call.
///
/// When `ssml` is `true` the engine interprets `text` as a valid SSML document
/// (W3C Speech Synthesis Markup Language 1.1). Engines that do not support SSML
/// MUST return `TtsError::SsmlUnsupported`.
#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    pub text: String,
    pub voice_id: VoiceId,
    /// Semitone shift, negative = lower. Valid range: [-12.0, 12.0].
    pub pitch_semitones: f32,
    /// Speech rate multiplier. Valid range: [0.25, 4.0].
    pub rate_multiplier: f32,
    pub ssml: bool,
}

/// Stable set of optional capabilities an engine may advertise.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub ssml: bool,
    pub neural_voices: bool,
    pub streaming: bool,
    pub custom_lexicons: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("synthesis timed out after {ms}ms")]
    Timeout { ms: u64 },

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("engine {id:?} is unavailable: {detail}")]
    EngineUnavailable { id: EngineId, detail: String },

    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("voice {id:?} is not recognized by this engine")]
    InvalidVoice { id: VoiceId },

    #[error("network failure: {0}")]
    NetworkFailed(String),

    #[error("SSML is not supported by engine {id:?}")]
    SsmlUnsupported { id: EngineId },

    #[error("engine I/O: {0}")]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    fn engine_id(&self) -> &EngineId;
    fn capabilities(&self) -> &EngineCapabilities;
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError>;
    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError>;
}

pub trait TtsEngineFactory: Send + Sync {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError>;
}

pub struct TtsRegistry {
    factories: HashMap<EngineId, Arc<dyn TtsEngineFactory>>,
}

impl TtsRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: EngineId, factory: Arc<dyn TtsEngineFactory>) {
        self.factories.insert(id, factory);
    }

    pub fn get(&self, id: &EngineId) -> Option<Arc<dyn TtsEngineFactory>> {
        self.factories.get(id).cloned()
    }

    pub fn engine_ids(&self) -> Vec<EngineId> {
        let mut ids: Vec<_> = self.factories.keys().cloned().collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids
    }
}

impl Default for TtsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _dyn_engine(_: &dyn TtsEngine) {}
    fn _dyn_factory(_: &dyn TtsEngineFactory) {}

    #[test]
    fn registry_register_and_lookup() {
        struct FakeFactory;
        impl TtsEngineFactory for FakeFactory {
            fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
                Err(TtsError::EngineUnavailable {
                    id: EngineId("fake".into()),
                    detail: "test".into(),
                })
            }
        }

        let mut reg = TtsRegistry::new();
        let id = EngineId("fake".into());
        reg.register(id.clone(), Arc::new(FakeFactory));
        assert!(reg.get(&id).is_some());
        assert_eq!(reg.engine_ids(), vec![EngineId("fake".into())]);
    }

    #[test]
    fn engine_ids_sorted() {
        struct FakeFactory;
        impl TtsEngineFactory for FakeFactory {
            fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
                Err(TtsError::EngineUnavailable {
                    id: EngineId("x".into()),
                    detail: "test".into(),
                })
            }
        }

        let mut reg = TtsRegistry::new();
        reg.register(EngineId("zzz".into()), Arc::new(FakeFactory));
        reg.register(EngineId("aaa".into()), Arc::new(FakeFactory));
        let ids = reg.engine_ids();
        assert_eq!(ids[0], EngineId("aaa".into()));
        assert_eq!(ids[1], EngineId("zzz".into()));
    }

    #[test]
    fn tts_voice_serde_roundtrip() {
        let voice = TtsVoice {
            id: VoiceId("uk_UA-ukrainian-medium".into()),
            name: "Ukrainian Medium".into(),
            locale: "uk-UA".into(),
            gender: VoiceGender::Neutral,
            engine_id: EngineId("piper".into()),
            is_neural: false,
            sample_rate_hint: 22_050,
        };
        let json = serde_json::to_string(&voice).expect("serialize");
        let back: TtsVoice = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, voice.id);
        assert_eq!(back.locale, voice.locale);
        assert_eq!(back.sample_rate_hint, 22_050);
    }
}
