pub mod error;
mod signer;
pub mod synth;
pub mod voices;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_platform_core::RateLimiter;
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsError, TtsVoice, VoiceId,
};

use crate::credentials::PollyCredentials;
use crate::policy::{RetryConfig, SynthesisRateLimiter, retry_synthesize};
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
            PollyError::SignatureError(msg) => TtsError::AuthFailed { reason: msg },
            PollyError::VoiceNotFound(id) => TtsError::InvalidVoice { id: VoiceId(id) },
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
    client: reqwest::Client,
    limiter: Arc<SynthesisRateLimiter>,
    retry_cfg: RetryConfig,
}

fn resolved_base_url(creds: &PollyCredentials) -> String {
    creds
        .base_url
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://polly.{}.amazonaws.com", creds.region))
}

impl PollyEngine {
    pub fn new(credentials: PollyCredentials) -> Self {
        Self {
            id: EngineId("polly".into()),
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

pub struct PollyEngineFactory {
    credentials: PollyCredentials,
}

impl PollyEngineFactory {
    pub fn new(credentials: PollyCredentials) -> Self {
        Self { credentials }
    }
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
        let base_url = resolved_base_url(&self.credentials);
        voices::fetch_voices(&self.client, &self.credentials, &base_url)
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let client = self.client.clone();
        let credentials = self.credentials.clone();
        let base_url = resolved_base_url(&self.credentials);
        let limiter = Arc::clone(&self.limiter) as Arc<dyn RateLimiter>;
        let cfg = self.retry_cfg;

        retry_synthesize(self.id.clone(), limiter, cfg, move || {
            let client = client.clone();
            let credentials = credentials.clone();
            let base_url = base_url.clone();
            let req = request.clone();
            async move {
                synth::synthesize(&client, &credentials, &base_url, req)
                    .await
                    .map_err(TtsError::from)
            }
        })
        .await
    }

    async fn test_connection(&self) -> Result<(), TtsError> {
        let base_url = resolved_base_url(&self.credentials);
        synth::probe_connection(&self.client, &self.credentials, &base_url)
            .await
            .map_err(TtsError::from)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_creds(base_url: &str) -> PollyCredentials {
        PollyCredentials {
            access_key_id: "AKID".into(),
            secret_access_key: "secret".into(),
            region: "us-east-1".into(),
            base_url: Some(base_url.to_string()),
        }
    }

    #[tokio::test]
    async fn test_connection_returns_ok_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{\"Voices\":[]}"))
            .mount(&server)
            .await;

        let engine = PollyEngine::new(test_creds(&server.uri()));
        assert!(engine.test_connection().await.is_ok());
    }

    #[tokio::test]
    async fn test_connection_auth_failed_on_403() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/voices"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let engine = PollyEngine::new(test_creds(&server.uri()));
        assert!(matches!(
            engine.test_connection().await,
            Err(TtsError::AuthFailed { .. })
        ));
    }
}
