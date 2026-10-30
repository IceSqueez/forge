pub mod error;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{
    ENGINE_ID_ELEVENLABS, EngineCapabilities, SynthesisRequest, TtsEngine, TtsError,
};

use crate::credentials::ElevenLabsCredentials;
use crate::network::{CloudGateways, ProviderGateway, ProviderLink};
use crate::policy::{RetryConfig, retry_synthesize};
use error::ElevenLabsError;

impl From<ElevenLabsError> for TtsError {
    fn from(e: ElevenLabsError) -> Self {
        let engine_id = EngineId(ENGINE_ID_ELEVENLABS.into());
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
            ElevenLabsError::Rejected(detail) | ElevenLabsError::Decode(detail) => {
                TtsError::SynthesisFailed {
                    id: engine_id,
                    detail,
                }
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

const DEFAULT_BASE_URL: &str = "https://api.elevenlabs.io";
pub(crate) const SAMPLE_RATE_HZ: u32 = 24_000;

pub struct ElevenLabsEngine {
    id: EngineId,
    credentials: ElevenLabsCredentials,
    link: ProviderLink,
    retry_cfg: RetryConfig,
}

impl ElevenLabsEngine {
    pub fn new(
        credentials: ElevenLabsCredentials,
        provider: &ProviderGateway,
    ) -> Result<Self, TtsError> {
        let id = EngineId(ENGINE_ID_ELEVENLABS.into());
        let base_url = credentials.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
        let link = provider
            .link(base_url)
            .map_err(|e| TtsError::EngineUnavailable {
                id: id.clone(),
                detail: e.to_string(),
            })?;
        Ok(Self {
            id,
            credentials,
            retry_cfg: link.retry_config(),
            link,
        })
    }
}

pub struct ElevenLabsEngineFactory {
    credentials: ElevenLabsCredentials,
    provider: ProviderGateway,
}

impl ElevenLabsEngineFactory {
    pub fn new(credentials: ElevenLabsCredentials, gateways: &CloudGateways) -> Self {
        Self {
            credentials,
            provider: gateways.provider(ENGINE_ID_ELEVENLABS),
        }
    }
}

impl forge_tts_core::TtsEngineFactory for ElevenLabsEngineFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Cloud
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(ElevenLabsEngine::new(
            self.credentials.clone(),
            &self.provider,
        )?))
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
        voices::fetch_voices(&self.link, self.credentials.api_key.expose())
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if request.ssml {
            return Err(TtsError::SsmlUnsupported {
                id: self.id.clone(),
            });
        }

        let link = self.link.clone();
        let api_key = self.credentials.api_key.clone();
        let cfg = self.retry_cfg.clone();

        retry_synthesize(self.id.clone(), cfg, move || {
            let link = link.clone();
            let api_key = api_key.clone();
            let req = request.clone();
            async move {
                synth::synthesize(&link, api_key.expose(), req)
                    .await
                    .map_err(TtsError::from)
            }
        })
        .await
    }

    async fn test_connection(&self) -> Result<(), TtsError> {
        synth::probe_connection(&self.link, self.credentials.api_key.expose())
            .await
            .map_err(TtsError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        API_KEY, OTHER_API_KEY, PAST_WAIT_THRESHOLD_SECS, is_the_provider_cool_down, request_count,
        serving, throttled,
    };
    use forge_speech_types::VoiceId;
    use forge_tts_core::TtsEngineFactory;
    use forge_types::Secret;
    use wiremock::ResponseTemplate;

    fn credentials(api_key: &str, base_url: &str) -> ElevenLabsCredentials {
        ElevenLabsCredentials {
            api_key: Secret::new(api_key),
            base_url: Some(base_url.into()),
        }
    }

    fn engine_at(base_url: &str) -> ElevenLabsEngine {
        ElevenLabsEngine::new(
            credentials(API_KEY, base_url),
            &CloudGateways::new().provider(ENGINE_ID_ELEVENLABS),
        )
        .unwrap()
    }

    fn request(ssml: bool) -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("voice-123".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml,
        }
    }

    #[tokio::test]
    async fn synthesize_refuses_ssml_without_a_request() {
        let server = serving(ResponseTemplate::new(200)).await;
        let result = engine_at(&server.uri()).synthesize(request(true)).await;
        assert!(matches!(result, Err(TtsError::SsmlUnsupported { .. })));
        assert_eq!(request_count(&server).await, 0);
    }

    #[test]
    fn engine_build_refuses_a_base_the_gateway_cannot_use() {
        for base in ["http://api.example.com", "api.elevenlabs.io"] {
            let result = ElevenLabsEngine::new(
                credentials(API_KEY, base),
                &CloudGateways::new().provider(ENGINE_ID_ELEVENLABS),
            );
            assert!(
                matches!(&result, Err(TtsError::EngineUnavailable { detail, .. }) if !detail.contains(API_KEY)),
                "{base}"
            );
        }
    }

    #[tokio::test]
    async fn a_rejected_request_fails_at_once_without_a_retry() {
        let server = serving(ResponseTemplate::new(400).set_body_string("malformed")).await;
        let result = engine_at(&server.uri()).synthesize(request(false)).await;
        assert!(
            matches!(result, Err(TtsError::SynthesisFailed { .. })),
            "{result:?}"
        );
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn a_quota_429_fails_at_once_without_a_retry() {
        let server =
            serving(ResponseTemplate::new(429).set_body_string(r#"{"status":"quota_exceeded"}"#))
                .await;
        let result = engine_at(&server.uri()).synthesize(request(false)).await;
        assert!(
            matches!(result, Err(TtsError::QuotaExceeded { .. })),
            "{result:?}"
        );
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn a_throttle_hint_past_the_wait_threshold_fails_at_once() {
        let server = serving(throttled(61)).await;
        let result = engine_at(&server.uri()).synthesize(request(false)).await;
        assert!(matches!(
            result,
            Err(TtsError::RateLimited {
                retry_after_secs: 61
            })
        ));
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn rebuilt_and_re_registered_engines_inherit_the_provider_cool_down() {
        let throttling = serving(throttled(PAST_WAIT_THRESHOLD_SECS)).await;
        let healthy = serving(ResponseTemplate::new(200)).await;
        let gateways = CloudGateways::new();
        let factory =
            ElevenLabsEngineFactory::new(credentials(API_KEY, &throttling.uri()), &gateways);
        let first = factory.create().unwrap();
        assert!(first.synthesize(request(false)).await.is_err());

        let re_registered = ElevenLabsEngineFactory::new(
            credentials(OTHER_API_KEY, &healthy.uri()),
            &gateways.clone(),
        );
        for engine in [factory.create().unwrap(), re_registered.create().unwrap()] {
            for result in [
                engine.synthesize(request(false)).await.map(|_| ()),
                engine.list_voices().await.map(|_| ()),
                engine.test_connection().await,
            ] {
                assert!(
                    matches!(
                        result,
                        Err(TtsError::RateLimited { retry_after_secs }) if is_the_provider_cool_down(retry_after_secs)
                    ),
                    "{result:?}"
                );
            }
        }
        assert_eq!(request_count(&throttling).await, 1);
        assert_eq!(request_count(&healthy).await, 0);
    }
}
