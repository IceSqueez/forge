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

use crate::credentials::AzureCredentials;
use crate::policy::{RetryConfig, SynthesisRateLimiter, retry_synthesize};
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
            AzureError::VoiceNotFound(id) => TtsError::InvalidVoice { id },
            AzureError::Decode(msg) => TtsError::NetworkFailed(msg),
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
    client: reqwest::Client,
    limiter: Arc<SynthesisRateLimiter>,
    retry_cfg: RetryConfig,
}

fn resolved_base_url(creds: &AzureCredentials) -> String {
    creds
        .base_url
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://{}.tts.speech.microsoft.com", creds.region))
}

impl AzureEngine {
    pub fn new(credentials: AzureCredentials) -> Self {
        Self {
            id: EngineId("azure".into()),
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

pub struct AzureEngineFactory {
    credentials: AzureCredentials,
}

impl AzureEngineFactory {
    pub fn new(credentials: AzureCredentials) -> Self {
        Self { credentials }
    }
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
        let base_url = resolved_base_url(&self.credentials);
        voices::fetch_voices(&self.client, &self.credentials.api_key, &base_url)
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let client = self.client.clone();
        let api_key = self.credentials.api_key.clone();
        let base_url = resolved_base_url(&self.credentials);
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
        let base_url = resolved_base_url(&self.credentials);
        synth::probe_connection(&self.client, &self.credentials.api_key, &base_url)
            .await
            .map_err(TtsError::from)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_returns_ok_on_200() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cognitiveservices/voices/list"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"[]"))
            .mount(&server)
            .await;

        let engine = AzureEngine::new(AzureCredentials {
            api_key: "test-key".into(),
            region: "eastus".into(),
            base_url: Some(server.uri()),
        });
        let result = engine.test_connection().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connection_returns_auth_failed_on_401() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cognitiveservices/voices/list"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let engine = AzureEngine::new(AzureCredentials {
            api_key: "bad-key".into(),
            region: "eastus".into(),
            base_url: Some(server.uri()),
        });
        let result = engine.test_connection().await;
        assert!(matches!(result, Err(TtsError::AuthFailed { .. })));
    }
}
