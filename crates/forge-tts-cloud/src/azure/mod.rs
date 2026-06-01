pub mod error;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsError, TtsVoice,
};

use crate::credentials::AzureCredentials;
use error::AzureError;

impl From<AzureError> for TtsError {
    fn from(e: AzureError) -> Self {
        let engine_id = EngineId("azure".into());
        match e {
            AzureError::Http(msg) => TtsError::NetworkFailed(msg),
            AzureError::Unauthorized(reason) => TtsError::AuthFailed { reason },
            AzureError::QuotaExceeded(detail) => TtsError::QuotaExceeded {
                id: engine_id,
                detail,
            },
            AzureError::RateLimited { retry_after_secs } => {
                TtsError::RateLimited { retry_after_secs }
            }
            AzureError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: true,
    neural_voices: true,
    streaming: false,
    custom_lexicons: false,
};

pub struct AzureEngine {
    id: EngineId,
    credentials: AzureCredentials,
}

impl AzureEngine {
    pub fn new(credentials: AzureCredentials) -> Self {
        Self {
            id: EngineId("azure".into()),
            credentials,
        }
    }
}

pub struct AzureEngineFactory {
    pub credentials: AzureCredentials,
}

impl forge_tts_core::TtsEngineFactory for AzureEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(AzureEngine::new(self.credentials.clone())))
    }
}

#[async_trait]
impl TtsEngine for AzureEngine {
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
