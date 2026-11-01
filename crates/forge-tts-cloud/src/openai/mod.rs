mod synth;
mod voices;

use async_trait::async_trait;
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{ENGINE_ID_OPENAI, SynthesisRequest, TtsEngine, TtsError};

use crate::credentials::OpenAiCredentials;
use crate::factory::{CloudEngine, CloudEngineFactory};
use crate::network::{ProviderGateway, ProviderLink};
use crate::policy::{RetryConfig, retry_synthesize};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";

pub struct OpenAiEngine {
    id: EngineId,
    credentials: OpenAiCredentials,
    link: ProviderLink,
    retry_cfg: RetryConfig,
}

impl OpenAiEngine {
    pub fn new(
        credentials: OpenAiCredentials,
        provider: &ProviderGateway,
    ) -> Result<Self, TtsError> {
        let id = EngineId(ENGINE_ID_OPENAI.into());
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

pub type OpenAiEngineFactory = CloudEngineFactory<OpenAiEngine>;

impl CloudEngine for OpenAiEngine {
    type Credentials = OpenAiCredentials;

    const ENGINE_ID: &'static str = ENGINE_ID_OPENAI;

    fn connect(
        credentials: Self::Credentials,
        provider: &ProviderGateway,
    ) -> Result<Self, TtsError> {
        Self::new(credentials, provider)
    }
}

#[async_trait]
impl TtsEngine for OpenAiEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(voices::static_voices())
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if request.text.is_ssml() {
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
                    .map_err(|error| error.into_tts(ENGINE_ID_OPENAI))
            }
        })
        .await
    }

    async fn test_connection(&self) -> Result<(), TtsError> {
        synth::probe_connection(&self.link, self.credentials.api_key.expose())
            .await
            .map_err(|error| error.into_tts(ENGINE_ID_OPENAI))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CloudGateways;
    use crate::network::test_support::{
        API_KEY, OTHER_API_KEY, PAST_WAIT_THRESHOLD_SECS, is_the_provider_cool_down, request_count,
        serving, throttled,
    };
    use forge_speech_types::VoiceId;
    use forge_tts_core::SpeechText;
    use forge_tts_core::TtsEngineFactory;
    use forge_types::Secret;
    use wiremock::ResponseTemplate;

    fn credentials(api_key: &str, base_url: &str) -> OpenAiCredentials {
        OpenAiCredentials {
            api_key: Secret::new(api_key),
            base_url: Some(base_url.into()),
        }
    }

    fn engine_at(base_url: &str) -> OpenAiEngine {
        OpenAiEngine::new(
            credentials(API_KEY, base_url),
            &CloudGateways::new().provider(ENGINE_ID_OPENAI),
        )
        .unwrap()
    }

    fn request(ssml: bool) -> SynthesisRequest {
        SynthesisRequest {
            text: SpeechText::new("hello".to_owned(), ssml),
            voice_id: VoiceId("alloy".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
        }
    }

    #[tokio::test]
    async fn synthesize_refuses_ssml_without_a_request() {
        let server = serving(ResponseTemplate::new(reqwest::StatusCode::OK)).await;
        let result = engine_at(&server.uri()).synthesize(request(true)).await;
        assert!(matches!(result, Err(TtsError::SsmlUnsupported { .. })));
        assert_eq!(request_count(&server).await, 0);
    }

    #[test]
    fn engine_build_refuses_a_base_the_gateway_cannot_use() {
        for base in ["http://api.example.com", "api.openai.com"] {
            let result = OpenAiEngine::new(
                credentials(API_KEY, base),
                &CloudGateways::new().provider(ENGINE_ID_OPENAI),
            );
            assert!(
                matches!(&result, Err(TtsError::EngineUnavailable { detail, .. }) if !detail.contains(API_KEY)),
                "{base}"
            );
        }
    }

    #[tokio::test]
    async fn a_rejected_request_fails_at_once_without_a_retry() {
        let server = serving(
            ResponseTemplate::new(reqwest::StatusCode::BAD_REQUEST).set_body_string("malformed"),
        )
        .await;
        let result = engine_at(&server.uri()).synthesize(request(false)).await;
        assert!(
            matches!(result, Err(TtsError::SynthesisFailed { .. })),
            "{result:?}"
        );
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn a_quota_429_fails_at_once_without_a_retry() {
        let server = serving(
            ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS)
                .set_body_string(r#"{"error":{"code":"insufficient_quota"}}"#),
        )
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
        let healthy = serving(ResponseTemplate::new(reqwest::StatusCode::OK)).await;
        let gateways = CloudGateways::new();
        let factory = OpenAiEngineFactory::new(credentials(API_KEY, &throttling.uri()), &gateways);
        let first = factory.create().unwrap();
        assert!(first.synthesize(request(false)).await.is_err());

        let re_registered = OpenAiEngineFactory::new(
            credentials(OTHER_API_KEY, &healthy.uri()),
            &gateways.clone(),
        );
        for engine in [factory.create().unwrap(), re_registered.create().unwrap()] {
            for result in [
                engine.synthesize(request(false)).await.map(|_| ()),
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
