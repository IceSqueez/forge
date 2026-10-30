pub mod error;
mod signer;
pub mod synth;
pub mod voices;

use async_trait::async_trait;
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice, VoiceId};
use forge_tts_core::{ENGINE_ID_POLLY, EngineCapabilities, SynthesisRequest, TtsEngine, TtsError};

use crate::credentials::PollyCredentials;
use crate::network::{CloudGateways, ProviderGateway, ProviderLink, is_dns_label};
use crate::policy::{RetryConfig, retry_synthesize};
use error::PollyError;
use signer::PollySigner;

pub(crate) const SAMPLE_RATE_HZ: u32 = 16_000;
const INVALID_REGION: &str = "AWS region must be a single region name such as us-east-1";

impl From<PollyError> for TtsError {
    fn from(e: PollyError) -> Self {
        let engine_id = EngineId(ENGINE_ID_POLLY.into());
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
            PollyError::Rejected(detail) | PollyError::Decode(detail) => {
                TtsError::SynthesisFailed {
                    id: engine_id,
                    detail,
                }
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
    signer: PollySigner,
    link: ProviderLink,
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
    pub fn new(
        credentials: PollyCredentials,
        provider: &ProviderGateway,
    ) -> Result<Self, TtsError> {
        let id = EngineId(ENGINE_ID_POLLY.into());
        if credentials.base_url.is_none() && !is_dns_label(&credentials.region) {
            return Err(TtsError::EngineUnavailable {
                id,
                detail: INVALID_REGION.into(),
            });
        }
        let base_url = resolved_base_url(&credentials);
        let link = provider
            .link(&base_url)
            .map_err(|e| TtsError::EngineUnavailable {
                id: id.clone(),
                detail: e.to_string(),
            })?;
        Ok(Self {
            id,
            signer: PollySigner::new(credentials),
            retry_cfg: link.retry_config(),
            link,
        })
    }
}

pub struct PollyEngineFactory {
    credentials: PollyCredentials,
    provider: ProviderGateway,
}

impl PollyEngineFactory {
    pub fn new(credentials: PollyCredentials, gateways: &CloudGateways) -> Self {
        Self {
            credentials,
            provider: gateways.provider(ENGINE_ID_POLLY),
        }
    }
}

impl forge_tts_core::TtsEngineFactory for PollyEngineFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Cloud
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(PollyEngine::new(
            self.credentials.clone(),
            &self.provider,
        )?))
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
        voices::fetch_voices(&self.link, &self.signer)
            .await
            .map_err(TtsError::from)
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let link = self.link.clone();
        let signer = self.signer.clone();
        let cfg = self.retry_cfg.clone();

        retry_synthesize(self.id.clone(), cfg, move || {
            let link = link.clone();
            let signer = signer.clone();
            let req = request.clone();
            async move {
                synth::synthesize(&link, &signer, req)
                    .await
                    .map_err(TtsError::from)
            }
        })
        .await
    }

    async fn test_connection(&self) -> Result<(), TtsError> {
        synth::probe_connection(&self.link, &self.signer)
            .await
            .map_err(TtsError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::test_support::{
        PAST_WAIT_THRESHOLD_SECS, freeze_clock, request_count, serving, throttled,
    };
    use crate::polly::signer::test_support::{
        POLLY_SERVICE, REGION, SECRET_ACCESS_KEY, WireRequest, credentials,
    };
    use forge_tts_core::TtsEngineFactory;
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn credentials_at(region: &str, base_url: Option<&str>) -> PollyCredentials {
        PollyCredentials {
            region: region.into(),
            ..credentials(base_url)
        }
    }

    fn engine_at(base_url: &str) -> PollyEngine {
        PollyEngine::new(
            credentials(Some(base_url)),
            &CloudGateways::new().provider(ENGINE_ID_POLLY),
        )
        .unwrap()
    }

    fn request() -> SynthesisRequest {
        SynthesisRequest {
            text: "hello".into(),
            voice_id: VoiceId("Joanna".into()),
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
            ("us-east-1", None, true),
            ("EU-WEST-1", None, true),
            (longest.as_str(), None, true),
            (too_long.as_str(), None, false),
            ("-us-east-1", None, false),
            ("us-east-1-", None, false),
            ("us.east-1", None, false),
            ("us-east-1/", None, false),
            ("evil.example/x", None, false),
            ("", None, false),
            ("us east 1", None, false),
            ("us-east-1?x=1", None, false),
            ("us-eäst-1", None, false),
            ("", Some("http://127.0.0.1:9"), true),
            ("us-east-1", Some("http://localhost:9"), true),
            ("us-east-1", Some("https://polly.example.com/prefix"), true),
            ("us-east-1", Some("http://polly.example.com"), false),
            ("us-east-1", Some("ftp://127.0.0.1"), false),
            ("us-east-1", Some("polly.example.com"), false),
        ] {
            let result = PollyEngine::new(
                credentials_at(region, base),
                &CloudGateways::new().provider(ENGINE_ID_POLLY),
            );
            if accepted {
                assert!(result.is_ok(), "{region:?} {base:?}");
            } else {
                assert!(
                    matches!(&result, Err(TtsError::EngineUnavailable { detail, .. }) if !detail.contains(SECRET_ACCESS_KEY)),
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
            serving(ResponseTemplate::new(429).set_body_string("Character quota exceeded")).await;
        let result = engine_at(&server.uri()).synthesize(request()).await;
        assert!(
            matches!(result, Err(TtsError::QuotaExceeded { .. })),
            "{result:?}"
        );
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn every_synthesis_retry_is_signed_afresh() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 4]))
            .mount(&server)
            .await;

        engine_at(&server.uri())
            .synthesize(request())
            .await
            .unwrap();

        let wire: Vec<WireRequest> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(WireRequest::from)
            .collect();
        assert_eq!(wire.len(), 2);
        assert!(wire[0].header("x-amz-date") < wire[1].header("x-amz-date"));
        assert_ne!(wire[0].sent_signature(), wire[1].sent_signature());
        for attempt in &wire {
            assert_eq!(
                attempt.sent_signature(),
                attempt.verified_signature(SECRET_ACCESS_KEY, REGION, POLLY_SERVICE)
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rebuilt_and_re_registered_engines_inherit_the_provider_cool_down() {
        let _clock = freeze_clock();
        let throttling = serving(throttled(PAST_WAIT_THRESHOLD_SECS)).await;
        let healthy = serving(ResponseTemplate::new(200)).await;
        let gateways = CloudGateways::new();
        let factory = PollyEngineFactory::new(credentials(Some(&throttling.uri())), &gateways);
        let first = factory.create().unwrap();
        assert!(first.synthesize(request()).await.is_err());

        let re_registered = PollyEngineFactory::new(
            credentials_at("eu-west-1", Some(&healthy.uri())),
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
