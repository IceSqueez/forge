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

use crate::credentials::OpenAiCredentials;
use crate::policy::{RetryConfig, SynthesisRateLimiter, retry_synthesize};
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
            OpenAiError::VoiceNotFound(id) => TtsError::InvalidVoice { id },
            OpenAiError::Decode(msg) => TtsError::NetworkFailed(msg),
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

const DEFAULT_BASE_URL: &str = "https://api.openai.com";

pub struct OpenAiEngine {
    id: EngineId,
    credentials: OpenAiCredentials,
    client: reqwest::Client,
    limiter: Arc<SynthesisRateLimiter>,
    retry_cfg: RetryConfig,
}

impl OpenAiEngine {
    pub fn new(credentials: OpenAiCredentials) -> Self {
        Self {
            id: EngineId("openai".into()),
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
        let engine = OpenAiEngine::new(OpenAiCredentials {
            api_key: "sk-test".into(),
            base_url: Some("http://localhost:0".into()),
        });
        let req = SynthesisRequest {
            text: "<speak>hello</speak>".into(),
            voice_id: VoiceId("alloy".into()),
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
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let engine = OpenAiEngine::new(OpenAiCredentials {
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
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}"))
            .mount(&server)
            .await;

        let engine = OpenAiEngine::new(OpenAiCredentials {
            api_key: "sk-valid".into(),
            base_url: Some(server.uri()),
        });
        let result = engine.test_connection().await;
        assert!(result.is_ok());
    }
}
