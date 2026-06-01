pub mod error;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsError, TtsVoice,
};

use crate::credentials::PollyCredentials;
use error::PollyError;

impl From<PollyError> for TtsError {
    fn from(e: PollyError) -> Self {
        let engine_id = EngineId("polly".into());
        match e {
            PollyError::Http(msg) => TtsError::NetworkFailed(msg),
            PollyError::Unauthorized(reason) => TtsError::AuthFailed { reason },
            PollyError::QuotaExceeded(detail) => TtsError::QuotaExceeded {
                id: engine_id,
                detail,
            },
            PollyError::RateLimited { retry_after_secs } => {
                TtsError::RateLimited { retry_after_secs }
            }
            PollyError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: true,
    neural_voices: true,
    streaming: false,
    custom_lexicons: false,
};

pub struct PollyEngine {
    id: EngineId,
    credentials: PollyCredentials,
}

impl PollyEngine {
    pub fn new(credentials: PollyCredentials) -> Self {
        Self {
            id: EngineId("polly".into()),
            credentials,
        }
    }
}

pub struct PollyEngineFactory {
    pub credentials: PollyCredentials,
}

impl forge_tts_core::TtsEngineFactory for PollyEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(PollyEngine::new(self.credentials.clone())))
    }
}

#[async_trait]
impl TtsEngine for PollyEngine {
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
        synth::synthesize(&self.credentials, request)
            .await
            .map_err(TtsError::from)
    }
}
