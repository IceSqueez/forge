pub mod error;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsError, TtsVoice,
};

use crate::credentials::OpenAiCredentials;
use error::OpenAiError;

impl From<OpenAiError> for TtsError {
    fn from(e: OpenAiError) -> Self {
        let engine_id = EngineId("openai".into());
        match e {
            OpenAiError::Http(msg) => TtsError::NetworkFailed(msg),
            OpenAiError::Unauthorized(reason) => TtsError::AuthFailed { reason },
            OpenAiError::QuotaExceeded(detail) => TtsError::QuotaExceeded {
                id: engine_id,
                detail,
            },
            OpenAiError::RateLimited { retry_after_secs } => {
                TtsError::RateLimited { retry_after_secs }
            }
            OpenAiError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: false,
    neural_voices: true,
    streaming: false,
    custom_lexicons: false,
};

pub struct OpenAiEngine {
    id: EngineId,
    credentials: OpenAiCredentials,
}

impl OpenAiEngine {
    pub fn new(credentials: OpenAiCredentials) -> Self {
        Self {
            id: EngineId("openai".into()),
            credentials,
        }
    }
}

pub struct OpenAiEngineFactory {
    pub credentials: OpenAiCredentials,
}

impl forge_tts_core::TtsEngineFactory for OpenAiEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(OpenAiEngine::new(self.credentials.clone())))
    }
}

#[async_trait]
impl TtsEngine for OpenAiEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    fn capabilities(&self) -> &EngineCapabilities {
        &CAPABILITIES
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(voices::static_voices())
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
