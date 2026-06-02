pub mod error;
pub mod synth;
pub mod voices;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_platform_core::RateLimiter;
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsError, TtsVoice,
};

use crate::credentials::ElevenLabsCredentials;
use crate::policy::{RetryConfig, SynthesisRateLimiter, retry_synthesize};
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
            ElevenLabsError::VoiceNotFound(id) => TtsError::InvalidVoice { id },
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

const DEFAULT_BASE_URL: &str = "https://api.elevenlabs.io";

pub struct ElevenLabsEngine {
    id: EngineId,
    credentials: ElevenLabsCredentials,
    client: reqwest::Client,
    limiter: Arc<SynthesisRateLimiter>,
    retry_cfg: RetryConfig,
}

impl ElevenLabsEngine {
    pub fn new(credentials: ElevenLabsCredentials) -> Self {
        Self {
            id: EngineId("elevenlabs".into()),
            credentials,
            client: reqwest::Client::new(),
            limiter: Arc::new(SynthesisRateLimiter::new()),
            retry_cfg: RetryConfig::default(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.retry_cfg.timeout = timeout;
        self
    }
}

pub struct ElevenLabsEngineFactory {
    credentials: ElevenLabsCredentials,
}

impl ElevenLabsEngineFactory {
    pub fn new(credentials: ElevenLabsCredentials) -> Self {
        Self { credentials }
    }
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
        let base_url = self
            .credentials
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE_URL);
        voices::fetch_voices(&self.client, &self.credentials.api_key, base_url)
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if request.ssml {
            return Err(TtsError::SsmlUnsupported {
                id: self.id.clone(),
            });
        }

        let client = self.client.clone();
        let api_key = self.credentials.api_key.clone();
        let base_url = self
            .credentials
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.into());
        let limiter = Arc::clone(&self.limiter) as Arc<dyn RateLimiter>;
        let cfg = self.retry_cfg;

        retry_synthesize(self.id.clone(), limiter, cfg, move || {
            let client = client.clone();
            let api_key = api_key.clone();
            let base_url = base_url.clone();
            let req = request.clone();
            async move {
                synth::synthesize(&client, &api_key, &base_url, req)
                    .await
                    .map_err(TtsError::from)
            }
        })
        .await
    }

    async fn test_connection(&self) -> Result<(), TtsError> {
        let base_url = self
            .credentials
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE_URL);
        synth::probe_connection(&self.client, &self.credentials.api_key, base_url)
            .await
            .map_err(TtsError::from)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_tts_core::VoiceId;

    #[tokio::test]
    async fn synthesize_returns_ssml_unsupported_without_http_call() {
        let engine = ElevenLabsEngine::new(ElevenLabsCredentials {
            api_key: "xi-test".into(),
            base_url: Some("http://localhost:0".into()),
        });
        let req = SynthesisRequest {
            text: "<speak>hello</speak>".into(),
            voice_id: VoiceId("abc".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: true,
        };
        let result = engine.synthesize(req).await;
        assert!(matches!(result, Err(TtsError::SsmlUnsupported { .. })));
    }

    #[tokio::test]
    async fn test_connection_maps_401_to_auth_failed() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/user"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let engine = ElevenLabsEngine::new(ElevenLabsCredentials {
            api_key: "bad-key".into(),
            base_url: Some(server.uri()),
        });
        let result = engine.test_connection().await;
        assert!(matches!(result, Err(TtsError::AuthFailed { .. })));
    }

    #[tokio::test]
    async fn test_connection_returns_ok_on_200() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/user"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}"))
            .mount(&server)
            .await;

        let engine = ElevenLabsEngine::new(ElevenLabsCredentials {
            api_key: "xi-valid".into(),
            base_url: Some(server.uri()),
        });
        let result = engine.test_connection().await;
        assert!(result.is_ok());
    }
}
