pub mod error;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{ENGINE_ID_AZURE, EngineCapabilities, SynthesisRequest, TtsEngine, TtsError};

use crate::credentials::AzureCredentials;
use crate::network::{CloudGateways, ProviderGateway, ProviderLink, is_dns_label};
use crate::policy::{RetryConfig, retry_synthesize};
use error::AzureError;

pub(crate) const SAMPLE_RATE_HZ: u32 = 24_000;
const INVALID_REGION: &str = "Azure region must be a single region name such as eastus";

impl From<AzureError> for TtsError {
    fn from(e: AzureError) -> Self {
        let engine_id = EngineId(ENGINE_ID_AZURE.into());
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
            AzureError::Rejected(detail) | AzureError::Decode(detail) => {
                TtsError::SynthesisFailed {
                    id: engine_id,
                    detail,
                }
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
    link: ProviderLink,
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
    pub fn new(
        credentials: AzureCredentials,
        provider: &ProviderGateway,
    ) -> Result<Self, TtsError> {
        let id = EngineId(ENGINE_ID_AZURE.into());
        if credentials.base_url.is_none() && !is_dns_label(&credentials.region) {
            return Err(TtsError::EngineUnavailable {
                id,
                detail: INVALID_REGION.into(),
            });
        }
        let link = provider
            .link(&resolved_base_url(&credentials))
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

pub struct AzureEngineFactory {
    credentials: AzureCredentials,
    provider: ProviderGateway,
}

impl AzureEngineFactory {
    pub fn new(credentials: AzureCredentials, gateways: &CloudGateways) -> Self {
        Self {
            credentials,
            provider: gateways.provider(ENGINE_ID_AZURE),
        }
    }
}

impl forge_tts_core::TtsEngineFactory for AzureEngineFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Cloud
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(AzureEngine::new(
            self.credentials.clone(),
            &self.provider,
        )?))
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
        voices::fetch_voices(&self.link, &self.credentials.api_key)
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let link = self.link.clone();
        let api_key = self.credentials.api_key.clone();
        let cfg = self.retry_cfg.clone();

        retry_synthesize(self.id.clone(), cfg, move || {
            let link = link.clone();
            let api_key = api_key.clone();
            let req = request.clone();
            async move {
                synth::synthesize(&link, &api_key, req)
                    .await
                    .map_err(TtsError::from)
            }
        })
        .await
    }

    async fn test_connection(&self) -> Result<(), TtsError> {
        synth::probe_connection(&self.link, &self.credentials.api_key)
            .await
            .map_err(TtsError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        API_KEY, OTHER_API_KEY, PAST_WAIT_THRESHOLD_SECS, freeze_clock, request_count, serving,
        throttled,
    };
    use forge_speech_types::VoiceId;
    use forge_tts_core::TtsEngineFactory;
    use wiremock::ResponseTemplate;

    fn credentials(api_key: &str, region: &str, base_url: Option<&str>) -> AzureCredentials {
        AzureCredentials {
            api_key: api_key.into(),
            region: region.into(),
            base_url: base_url.map(str::to_owned),
        }
    }

    fn engine_at(base_url: &str) -> AzureEngine {
        AzureEngine::new(
            credentials(API_KEY, "eastus", Some(base_url)),
            &CloudGateways::new().provider(ENGINE_ID_AZURE),
        )
        .unwrap()
    }

    fn request() -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("en-US-AriaNeural".into()),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        }
    }

    #[test]
    fn engine_build_accepts_only_a_single_dns_label_region_unless_a_base_overrides_it() {
        let longest = "a".repeat(63);
        let too_long = "a".repeat(64);
        for (region, base, accepted) in [
            ("a", None, true),
            ("eastus", None, true),
            ("east-us-2", None, true),
            ("EastUS", None, true),
            (longest.as_str(), None, true),
            (too_long.as_str(), None, false),
            ("-eastus", None, false),
            ("eastus-", None, false),
            ("east.us", None, false),
            ("east/us", None, false),
            ("evil.example/", None, false),
            ("", None, false),
            ("east us", None, false),
            ("eastus?x=1", None, false),
            ("eastüs", None, false),
            ("", Some("http://127.0.0.1:9"), true),
            ("eastus", Some("http://speech.example.com"), false),
            ("eastus", Some("speech.example.com"), false),
        ] {
            let result = AzureEngine::new(
                credentials(API_KEY, region, base),
                &CloudGateways::new().provider(ENGINE_ID_AZURE),
            );
            if accepted {
                assert!(result.is_ok(), "{region:?} {base:?}");
            } else {
                assert!(
                    matches!(&result, Err(TtsError::EngineUnavailable { detail, .. }) if !detail.contains(API_KEY)),
                    "{region:?} {base:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_rejected_request_fails_at_once_without_a_retry() {
        let server = serving(ResponseTemplate::new(400).set_body_string("malformed")).await;
        let result = engine_at(&server.uri()).synthesize(request()).await;
        assert!(
            matches!(result, Err(TtsError::SynthesisFailed { .. })),
            "{result:?}"
        );
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn a_quota_429_fails_at_once_without_a_retry() {
        let server =
            serving(ResponseTemplate::new(429).set_body_string("Quota Exceeded for this resource"))
                .await;
        let result = engine_at(&server.uri()).synthesize(request()).await;
        assert!(
            matches!(result, Err(TtsError::QuotaExceeded { .. })),
            "{result:?}"
        );
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn a_throttle_hint_past_the_wait_threshold_fails_at_once() {
        let server = serving(throttled(61)).await;
        let result = engine_at(&server.uri()).synthesize(request()).await;
        assert!(matches!(
            result,
            Err(TtsError::RateLimited {
                retry_after_secs: 61
            })
        ));
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn rebuilt_and_re_registered_engines_inherit_the_provider_cool_down() {
        let _clock = freeze_clock();
        let throttling = serving(throttled(PAST_WAIT_THRESHOLD_SECS)).await;
        let healthy = serving(ResponseTemplate::new(200)).await;
        let gateways = CloudGateways::new();
        let factory = AzureEngineFactory::new(
            credentials(API_KEY, "eastus", Some(&throttling.uri())),
            &gateways,
        );
        let first = factory.create().unwrap();
        assert!(first.synthesize(request()).await.is_err());

        let re_registered = AzureEngineFactory::new(
            credentials(OTHER_API_KEY, "westeurope", Some(&healthy.uri())),
            &gateways.clone(),
        );
        for engine in [factory.create().unwrap(), re_registered.create().unwrap()] {
            for result in [
                engine.synthesize(request()).await.map(|_| ()),
                engine.list_voices().await.map(|_| ()),
                engine.test_connection().await,
            ] {
                assert!(
                    matches!(
                        result,
                        Err(TtsError::RateLimited { retry_after_secs }) if retry_after_secs == PAST_WAIT_THRESHOLD_SECS
                    ),
                    "{result:?}"
                );
            }
        }
        assert_eq!(request_count(&throttling).await, 1);
        assert_eq!(request_count(&healthy).await, 0);
    }
}
