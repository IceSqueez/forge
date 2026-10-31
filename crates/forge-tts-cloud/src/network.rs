use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use forge_net::{
    CallClass, Gateway, GatewayBuildError, GatewayFailure, GatewayProfile, GatewayRequest,
    GatewayResponse, GatewayTimings, RateLimiter, ResolvedTarget, ThrottleHintFamily,
};

use crate::policy::{RetryConfig, SynthesisRateLimiter};

const NO_HINT_THROTTLE_WAIT: Duration = Duration::from_secs(1);
const MAX_DNS_LABEL_LEN: usize = 63;
const LABEL_HYPHEN: char = '-';
const SEGMENT_UNDERSCORE: char = '_';

pub(crate) const CLOUD_TTS_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Media,
    ThrottleHintFamily::RetryAfter,
    NO_HINT_THROTTLE_WAIT,
);

#[derive(Clone, Default)]
pub struct CloudGateways {
    providers: Arc<Mutex<HashMap<&'static str, ProviderGateway>>>,
    timings: GatewayTimings,
}

impl CloudGateways {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timings(timings: GatewayTimings) -> Self {
        Self {
            timings,
            ..Self::default()
        }
    }

    pub fn provider(&self, provider: &'static str) -> ProviderGateway {
        lock(&self.providers)
            .entry(provider)
            .or_insert_with(|| ProviderGateway::new(self.timings))
            .clone()
    }
}

impl fmt::Debug for CloudGateways {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut providers: Vec<&'static str> = lock(&self.providers).keys().copied().collect();
        providers.sort_unstable();
        f.debug_struct("CloudGateways")
            .field("providers", &providers)
            .finish()
    }
}

#[derive(Clone)]
pub struct ProviderGateway {
    limiter: Arc<SynthesisRateLimiter>,
    bound: Arc<Mutex<Option<BoundGateway>>>,
    timings: GatewayTimings,
}

struct BoundGateway {
    base_url: String,
    gateway: Gateway,
}

impl ProviderGateway {
    fn new(timings: GatewayTimings) -> Self {
        Self {
            limiter: Arc::new(SynthesisRateLimiter::new()),
            bound: Arc::new(Mutex::new(None)),
            timings,
        }
    }

    pub fn link(&self, base_url: &str) -> Result<ProviderLink, GatewayBuildError> {
        let mut bound = lock(&self.bound);
        if let Some(current) = bound.as_ref().filter(|b| b.base_url == base_url) {
            return Ok(self.link_over(current.gateway.clone()));
        }
        let gateway = Gateway::new(
            base_url,
            Arc::clone(&self.limiter) as Arc<dyn RateLimiter>,
            CLOUD_TTS_PROFILE.with_timings(self.timings),
            None,
        )?;
        *bound = Some(BoundGateway {
            base_url: base_url.to_owned(),
            gateway: gateway.clone(),
        });
        Ok(self.link_over(gateway))
    }

    fn link_over(&self, gateway: Gateway) -> ProviderLink {
        ProviderLink {
            gateway,
            limiter: Arc::clone(&self.limiter),
        }
    }
}

#[derive(Clone)]
pub struct ProviderLink {
    gateway: Gateway,
    limiter: Arc<SynthesisRateLimiter>,
}

impl ProviderLink {
    pub async fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, GatewayFailure> {
        self.gateway.send(request).await
    }

    pub fn resolve_path(&self, path: &str) -> Result<ResolvedTarget, GatewayFailure> {
        self.gateway.resolve_path(path)
    }

    pub fn catalog_budget(&self) -> Duration {
        self.gateway
            .profile()
            .timings
            .total_budget(CallClass::Catalog)
    }

    pub fn throttle_secs(&self, failure: &GatewayFailure) -> Option<u64> {
        match failure {
            GatewayFailure::RateLimited { retry_after, .. } => Some(ceil_secs(*retry_after)),
            GatewayFailure::BudgetExhausted { .. } => {
                Some(ceil_secs(self.limiter.cooldown_remaining()))
            }
            _ => None,
        }
    }

    pub fn retry_config(&self) -> RetryConfig {
        RetryConfig::paced_by(Arc::clone(&self.limiter))
    }
}

pub(crate) fn is_plain_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == LABEL_HYPHEN || c == SEGMENT_UNDERSCORE)
}

pub(crate) fn is_dns_label(value: &str) -> bool {
    value.len() <= MAX_DNS_LABEL_LEN
        && !value.starts_with(LABEL_HYPHEN)
        && !value.ends_with(LABEL_HYPHEN)
        && !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == LABEL_HYPHEN)
}

pub(crate) fn ceil_secs(wait: Duration) -> u64 {
    wait.as_secs()
        .saturating_add(u64::from(wait.subsec_nanos() > 0))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_net::{
        BodyExcerpt, CallClass, Gateway, GatewayFailure, GatewayTimings, HostRejection,
        InvalidRequest, ThrottleScope, TokenBucketRateLimiter,
    };
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{CLOUD_TTS_PROFILE, CloudGateways, ProviderLink};
    use crate::policy::SynthesisRateLimiter;
    use crate::policy::rate_limit::EXHAUSTED_THRESHOLD;

    pub(crate) const API_KEY: &str = "sk-SECRET-7f3a91";
    pub(crate) const OTHER_API_KEY: &str = "sk-OTHER-b2c4d6";
    pub(crate) const PAST_WAIT_THRESHOLD_SECS: u64 = 120;
    pub(crate) const NO_HINT_WAIT_SECS: u64 = 1;
    const UNPACED_BURST: u32 = 1_000;

    pub(crate) fn is_the_provider_cool_down(retry_after_secs: u64) -> bool {
        (EXHAUSTED_THRESHOLD.as_secs()..=PAST_WAIT_THRESHOLD_SECS).contains(&retry_after_secs)
    }

    pub(crate) async fn serving(template: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    pub(crate) fn link_to(server: &MockServer) -> ProviderLink {
        CloudGateways::new()
            .provider("test")
            .link(&server.uri())
            .unwrap()
    }

    pub(crate) fn unpaced_link_to(server: &MockServer) -> ProviderLink {
        unpaced_link_with(server, GatewayTimings::DEFAULT)
    }

    pub(crate) fn link_with_catalog_budget(server: &MockServer, budget: Duration) -> ProviderLink {
        unpaced_link_with(
            server,
            GatewayTimings {
                catalog_total: budget,
                ..GatewayTimings::DEFAULT
            },
        )
    }

    fn unpaced_link_with(server: &MockServer, timings: GatewayTimings) -> ProviderLink {
        ProviderLink {
            gateway: Gateway::new(
                &server.uri(),
                Arc::new(TokenBucketRateLimiter::new(
                    UNPACED_BURST,
                    Duration::from_secs(1),
                )),
                CLOUD_TTS_PROFILE.with_timings(timings),
                None,
            )
            .unwrap(),
            limiter: Arc::new(SynthesisRateLimiter::new()),
        }
    }

    pub(crate) async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    pub(crate) fn every_failure_without_a_body() -> Vec<(&'static str, GatewayFailure)> {
        let excerpt = BodyExcerpt::default;
        vec![
            ("401", GatewayFailure::Unauthorized { excerpt: excerpt() }),
            ("403", GatewayFailure::Forbidden { excerpt: excerpt() }),
            ("404", GatewayFailure::NotFound { excerpt: excerpt() }),
            (
                "429",
                GatewayFailure::RateLimited {
                    status: 429,
                    retry_after: Duration::from_secs(3),
                    scope: ThrottleScope::Bucket,
                    excerpt: excerpt(),
                },
            ),
            (
                "400",
                GatewayFailure::ClientRejected {
                    status: 400,
                    excerpt: excerpt(),
                },
            ),
            (
                "402",
                GatewayFailure::ClientRejected {
                    status: 402,
                    excerpt: excerpt(),
                },
            ),
            (
                "408",
                GatewayFailure::ClientRejected {
                    status: 408,
                    excerpt: excerpt(),
                },
            ),
            (
                "418",
                GatewayFailure::ClientRejected {
                    status: 418,
                    excerpt: excerpt(),
                },
            ),
            (
                "500",
                GatewayFailure::ServerError {
                    status: 500,
                    excerpt: excerpt(),
                },
            ),
            (
                "302",
                GatewayFailure::UnexpectedStatus {
                    status: 302,
                    excerpt: excerpt(),
                },
            ),
            (
                "budget",
                GatewayFailure::BudgetExhausted { retry_after: None },
            ),
            ("too-large", GatewayFailure::ResponseTooLarge { cap: 1 }),
            ("cancelled", GatewayFailure::Cancelled),
            (
                "address",
                GatewayFailure::AddressRefused(HostRejection::PrivateOrSpecial),
            ),
            (
                "invalid",
                GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl),
            ),
        ]
    }

    pub(crate) fn throttled(retry_after_secs: u64) -> ResponseTemplate {
        ResponseTemplate::new(429).insert_header("Retry-After", retry_after_secs.to_string())
    }

    pub(crate) fn over_catalog_cap() -> Vec<u8> {
        vec![0_u8; usize::try_from(CallClass::Catalog.body_cap()).unwrap() + 2]
    }

    pub(crate) fn catalog_cap_text() -> String {
        CallClass::Catalog.body_cap().to_string()
    }

    pub(crate) fn assert_reveals_nothing(text: &str, server: &MockServer) {
        assert!(!text.contains(API_KEY), "key leaked: {text}");
        assert!(
            !text.contains(&server.address().to_string()),
            "address leaked: {text}"
        );
        assert!(!text.contains("http://"), "URL leaked: {text}");
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{
        PAST_WAIT_THRESHOLD_SECS, is_the_provider_cool_down, request_count, serving, throttled,
    };
    use super::*;
    use forge_net::{BodyExcerpt, InvalidRequest, ThrottleScope};
    use wiremock::ResponseTemplate;

    #[test]
    fn ceil_secs_rounds_any_fraction_up_and_saturates() {
        for (wait, expected) in [
            (Duration::ZERO, 0),
            (Duration::from_nanos(1), 1),
            (Duration::from_secs(1), 1),
            (Duration::from_secs(1) + Duration::from_nanos(1), 2),
            (Duration::from_millis(59_001), 60),
            (Duration::MAX, u64::MAX),
        ] {
            assert_eq!(ceil_secs(wait), expected, "{wait:?}");
        }
    }

    #[tokio::test]
    async fn throttle_secs_answers_only_for_throttle_failures() {
        let link = CloudGateways::new()
            .provider("p")
            .link("https://api.example.com")
            .unwrap();
        let rate_limited = |retry_after| GatewayFailure::RateLimited {
            status: 429,
            retry_after,
            scope: ThrottleScope::Bucket,
            excerpt: BodyExcerpt::default(),
        };
        for (failure, expected) in [
            (rate_limited(Duration::from_millis(1_500)), Some(2)),
            (rate_limited(Duration::ZERO), Some(0)),
            (
                GatewayFailure::BudgetExhausted { retry_after: None },
                Some(0),
            ),
            (
                GatewayFailure::ServerError {
                    status: 503,
                    excerpt: BodyExcerpt::default(),
                },
                None,
            ),
            (
                GatewayFailure::InvalidRequest(InvalidRequest::MalformedUrl),
                None,
            ),
            (GatewayFailure::Cancelled, None),
        ] {
            assert_eq!(link.throttle_secs(&failure), expected, "{failure:?}");
        }
    }

    #[tokio::test]
    async fn a_provider_cool_down_reaches_every_link_of_that_provider() {
        let first = serving(throttled(PAST_WAIT_THRESHOLD_SECS)).await;
        let second = serving(ResponseTemplate::new(200)).await;
        let gateways = CloudGateways::new();
        let throttled_link = gateways.provider("p").link(&first.uri()).unwrap();
        assert!(matches!(
            throttled_link.send(GatewayRequest::get("x")).await,
            Err(GatewayFailure::RateLimited { .. })
        ));

        for link in [
            gateways.provider("p").link(&first.uri()).unwrap(),
            gateways.clone().provider("p").link(&second.uri()).unwrap(),
        ] {
            let failure = link.send(GatewayRequest::get("x")).await.unwrap_err();
            assert_eq!(
                failure,
                GatewayFailure::BudgetExhausted { retry_after: None }
            );
            assert!(
                link.throttle_secs(&failure)
                    .is_some_and(is_the_provider_cool_down),
                "{failure:?}"
            );
        }
        assert_eq!(request_count(&first).await, 1);
        assert_eq!(request_count(&second).await, 0);
    }

    #[tokio::test]
    async fn a_provider_cool_down_leaves_other_providers_untouched() {
        let throttling = serving(throttled(PAST_WAIT_THRESHOLD_SECS)).await;
        let healthy = serving(ResponseTemplate::new(200)).await;
        let gateways = CloudGateways::new();
        let throttled_link = gateways.provider("p").link(&throttling.uri()).unwrap();
        assert!(matches!(
            throttled_link.send(GatewayRequest::get("x")).await,
            Err(GatewayFailure::RateLimited { .. })
        ));

        let other = gateways.provider("q").link(&healthy.uri()).unwrap();

        assert!(other.send(GatewayRequest::get("x")).await.is_ok());
    }
}
