use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use forge_audio::PcmBuffer;

pub type Locale = String;

pub const ENGINE_ID_PIPER: &str = "piper";
pub const ENGINE_ID_ESPEAK_NG: &str = "espeak-ng";
pub const ENGINE_ID_SAPI: &str = "sapi";
pub const ENGINE_ID_NSSPEECH: &str = "nsspeech";
pub const ENGINE_ID_AZURE: &str = "azure";
pub const ENGINE_ID_ELEVENLABS: &str = "elevenlabs";
pub const ENGINE_ID_OPENAI: &str = "openai";
pub const ENGINE_ID_POLLY: &str = "polly";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineId(pub String);

impl EngineId {
    pub fn display_name(&self) -> &str {
        engine_display_name(&self.0)
    }
}

pub fn engine_display_name(id: &str) -> &str {
    match id {
        ENGINE_ID_PIPER => "Piper",
        ENGINE_ID_ESPEAK_NG => "eSpeak-NG",
        ENGINE_ID_SAPI => "Microsoft SAPI 5",
        ENGINE_ID_NSSPEECH => "Apple AVSpeech",
        ENGINE_ID_AZURE => "Azure Speech",
        ENGINE_ID_ELEVENLABS => "ElevenLabs",
        ENGINE_ID_OPENAI => "OpenAI TTS",
        ENGINE_ID_POLLY => "Amazon Polly",
        other => other,
    }
}

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

#[derive(Debug, Clone)]
pub struct SynthesisRequest {
    pub text: String,
    pub voice_id: VoiceId,
    pub pitch_semitones: f32,
    pub rate_multiplier: f32,
    pub ssml: bool,
}

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

    #[error("quota exhausted for engine {id:?}: {detail}")]
    QuotaExceeded { id: EngineId, detail: String },
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    fn engine_id(&self) -> &EngineId;
    fn capabilities(&self) -> &EngineCapabilities;
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError>;
    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError>;

    async fn test_connection(&self) -> Result<(), TtsError> {
        self.list_voices().await.map(|_| ())
    }
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
    fn engine_ids_show_their_product_names_and_unknown_ids_show_as_is() {
        for (id, shown) in [
            ("piper", "Piper"),
            ("espeak-ng", "eSpeak-NG"),
            ("sapi", "Microsoft SAPI 5"),
            ("nsspeech", "Apple AVSpeech"),
            ("azure", "Azure Speech"),
            ("elevenlabs", "ElevenLabs"),
            ("openai", "OpenAI TTS"),
            ("polly", "Amazon Polly"),
            ("avfoundation", "avfoundation"),
            ("my-engine", "my-engine"),
            ("", ""),
        ] {
            assert_eq!(EngineId(id.to_owned()).display_name(), shown, "{id:?}");
        }
    }

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
}
