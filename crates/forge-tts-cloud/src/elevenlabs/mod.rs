pub mod error;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsError, TtsVoice,
};

use crate::credentials::ElevenLabsCredentials;
use error::ElevenLabsError;

impl From<ElevenLabsError> for TtsError {
    fn from(e: ElevenLabsError) -> Self {
        let engine_id = EngineId("elevenlabs".into());
        match e {
            ElevenLabsError::Http(msg) => TtsError::NetworkFailed(msg),
            ElevenLabsError::Unauthorized(reason) => TtsError::AuthFailed { reason },
            ElevenLabsError::QuotaExceeded(detail) => TtsError::QuotaExceeded {
                id: engine_id,
                detail,
            },
            ElevenLabsError::RateLimited { retry_after_secs } => {
                TtsError::RateLimited { retry_after_secs }
            }
            ElevenLabsError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: false,
    neural_voices: true,
    streaming: false,
    custom_lexicons: false,
};

pub struct ElevenLabsEngine {
    id: EngineId,
    credentials: ElevenLabsCredentials,
}

impl ElevenLabsEngine {
    pub fn new(credentials: ElevenLabsCredentials) -> Self {
        Self {
            id: EngineId("elevenlabs".into()),
            credentials,
        }
    }
}

pub struct ElevenLabsEngineFactory {
    pub credentials: ElevenLabsCredentials,
}

impl forge_tts_core::TtsEngineFactory for ElevenLabsEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(ElevenLabsEngine::new(self.credentials.clone())))
    }
}

#[async_trait]
impl TtsEngine for ElevenLabsEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    fn capabilities(&self) -> &EngineCapabilities {
        &CAPABILITIES
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        voices::fetch_voices(&self.credentials)
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if request.ssml {
            return Err(TtsError::SsmlUnsupported {
                id: self.id.clone(),
            });
        }
        synth::synthesize(&self.credentials, request)
            .await
            .map_err(TtsError::from)
    }
}
