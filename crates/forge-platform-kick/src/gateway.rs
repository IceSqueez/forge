use std::sync::Arc;
use std::time::Duration;

use forge_events::{EventPublisher, EventSource, RequestFailed, TypedEvent};
use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FailureContext, FailureObserver, Gateway,
    GatewayBuildError, GatewayFailure, GatewayProfile, GatewayRequest, RateLimiter, RequestBody,
    ThrottleHintFamily,
};
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};
use forge_types::Secret;
use serde::Serialize;

pub const KICK_API_BUDGET_CAPACITY: u32 = 60;
pub const KICK_API_BUDGET_WINDOW: Duration = Duration::from_secs(60);

pub const KICK_CHANNEL_INFO_BUDGET_CAPACITY: u32 = 1;
pub const KICK_CHANNEL_INFO_BUDGET_WINDOW: Duration = CHANNEL_INFO_REFRESH_INTERVAL;

pub(crate) const CHANNEL_INFO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);
const BODY_SNIPPET_MAX_CHARS: usize = 200;

pub const KICK_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn kick_api_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    kick_gateway(endpoints, EndpointSurface::KickPublicApi, limiter, bus)
}

pub fn kick_channel_info_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    kick_gateway(endpoints, EndpointSurface::KickChannelApi, limiter, bus)
}

fn kick_gateway(
    endpoints: &PlatformEndpoints,
    surface: EndpointSurface,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(surface),
        limiter,
        KICK_GATEWAY_PROFILE,
        Some(Arc::new(RequestFailPublisher { bus })),
    )
}

struct RequestFailPublisher {
    bus: Arc<dyn EventPublisher>,
}

impl FailureObserver for RequestFailPublisher {
    fn on_failure(&self, failure: &GatewayFailure, context: FailureContext) {
        let Some(status) = failure.status() else {
            return;
        };
        let retry_after_secs = match failure {
            GatewayFailure::RateLimited { retry_after, .. } => Some(retry_after.as_secs()),
            _ => None,
        };
        let body_snippet: String = failure
            .excerpt()
            .map(|excerpt| {
                excerpt
                    .as_str()
                    .chars()
                    .take(BODY_SNIPPET_MAX_CHARS)
                    .collect()
            })
            .unwrap_or_default();
        self.bus.publish(
            RequestFailed {
                endpoint: context.label.unwrap_or_default().to_owned(),
                status_code: status,
                body_snippet,
                retry_after_secs,
            }
            .into_event(EventSource::Kick, None),
        );
    }
}

pub(crate) fn authorized(
    request: GatewayRequest,
    token: &str,
    label: &'static str,
) -> GatewayRequest {
    request.bearer(Secret::new(token)).label(label)
}

pub(crate) fn json_body<T: Serialize + ?Sized>(value: &T) -> Result<RequestBody, PlatformError> {
    RequestBody::json(value).map_err(|error| PlatformError::Network {
        reason: error.to_string(),
    })
}

pub(crate) fn malformed(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::MalformedResponse {
        reason: error.to_string(),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RefusalReasons {
    pub(crate) unauthorized: &'static str,
    pub(crate) forbidden: Option<&'static str>,
}

pub(crate) fn platform_error(failure: GatewayFailure, reasons: RefusalReasons) -> PlatformError {
    match (failure, reasons.forbidden) {
        (GatewayFailure::Unauthorized { .. }, _) => PlatformError::Auth {
            reason: reasons.unauthorized.to_owned(),
        },
        (GatewayFailure::Forbidden { .. }, Some(forbidden)) => PlatformError::Auth {
            reason: forbidden.to_owned(),
        },
        (other, _) => gateway_error(other),
    }
}

pub(crate) fn gateway_error(failure: GatewayFailure) -> PlatformError {
    match failure {
        GatewayFailure::RateLimited { retry_after, .. } => PlatformError::RateLimited {
            retry_after_secs: u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX),
        },
        GatewayFailure::BudgetExhausted { .. } => PlatformError::RateLimitExhausted,
        other => match (other.status(), other.excerpt()) {
            (Some(status), Some(excerpt)) => PlatformError::Http {
                status,
                body: excerpt.as_str().to_owned(),
            },
            _ => PlatformError::Network {
                reason: other.to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_net::{RateLimitError, RateLimitOutcome, TokenBucketRateLimiter};
    use wiremock::matchers::{any, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::api::categories::KickCategories;
    use crate::api::channel::KickChannel;
    use crate::api::chat_messages::KickSendChat;
    use crate::api::moderation::KickModeration;
    use crate::api::rewards::{CreateRewardParams, KickRewards};
    use crate::endpoints_test_support::{
        ExhaustedLimiter, RecordingPublisher, api_at, api_limited_by, api_with, channel_info_with,
    };
    use crate::pusher::error::KickError;

    const TOKEN: &str = "kick-secret-token-9f3a";

    type Outcome = Result<(), PlatformError>;

    struct Clients {
        send: KickSendChat,
        categories: KickCategories,
        channel: KickChannel,
        moderation: KickModeration,
        rewards: KickRewards,
    }

    impl Clients {
        fn over(api: &Gateway) -> Self {
            Self {
                send: KickSendChat::new(api.clone()),
                categories: KickCategories::new(api.clone()),
                channel: KickChannel::new(api.clone()),
                moderation: KickModeration::new(
                    api.clone(),
                    crate::ban_ledger_test_support::MemoryBanLedger::shared(),
                ),
                rewards: KickRewards::new(api.clone()),
            }
        }

        async fn every_call(&self) -> Vec<(&'static str, Outcome)> {
            vec![
                ("send", self.send.send("hi", TOKEN, 42, false).await),
                ("delete chat", self.send.delete("msg-1", TOKEN).await),
                (
                    "categories",
                    self.categories.search(TOKEN, "q").await.map(drop),
                ),
                (
                    "channel update",
                    self.channel
                        .update_info(TOKEN, Some("t".to_owned()), None, None)
                        .await,
                ),
                (
                    "channel get",
                    self.channel.get_channel(TOKEN).await.map(drop),
                ),
                ("ban", self.moderation.ban(99, 42, None, None, TOKEN).await),
                ("unban", self.moderation.unban(99, 42, TOKEN).await),
                (
                    "reward create",
                    self.rewards.create(reward(), TOKEN).await.map(drop),
                ),
                (
                    "pending redemptions",
                    self.rewards.list_pending_redemptions(TOKEN).await.map(drop),
                ),
            ]
        }
    }

    fn reward() -> CreateRewardParams {
        CreateRewardParams {
            title: "Hydrate".to_owned(),
            cost: 500,
            description: None,
            background_color: None,
            is_enabled: None,
            is_user_input_required: None,
            should_redemptions_skip_request_queue: None,
        }
    }

    async fn answering(template: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    struct GrantsThenExhausts {
        remaining: Mutex<usize>,
    }

    impl GrantsThenExhausts {
        fn granting(grants: usize) -> Self {
            Self {
                remaining: Mutex::new(grants),
            }
        }
    }

    #[async_trait]
    impl RateLimiter for GrantsThenExhausts {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
            let mut remaining = self.remaining.lock().unwrap();
            if *remaining == 0 {
                return Ok(RateLimitOutcome::Exhausted);
            }
            *remaining -= 1;
            Ok(RateLimitOutcome::Granted)
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn official_bucket() -> Arc<dyn RateLimiter> {
        Arc::new(TokenBucketRateLimiter::new(
            KICK_API_BUDGET_CAPACITY,
            KICK_API_BUDGET_WINDOW,
        ))
    }

    fn channel_info_bucket() -> Arc<dyn RateLimiter> {
        Arc::new(TokenBucketRateLimiter::new(
            KICK_CHANNEL_INFO_BUDGET_CAPACITY,
            KICK_CHANNEL_INFO_BUDGET_WINDOW,
        ))
    }

    #[tokio::test]
    async fn every_success_status_counts_as_success_for_every_client() {
        for status in [200_u16, 201, 202] {
            let server =
                answering(ResponseTemplate::new(status).set_body_string(r#"{"data":[{}]}"#)).await;
            let clients = Clients::over(&api_at(&server.uri()));

            for (label, outcome) in clients.every_call().await {
                assert!(outcome.is_ok(), "{label} on {status}: {outcome:?}");
            }
        }
    }

    #[tokio::test]
    async fn no_content_counts_as_success_for_every_call_that_reads_no_body() {
        let reads_a_body = ["categories", "channel get", "pending redemptions"];
        let server = answering(ResponseTemplate::new(204)).await;
        let clients = Clients::over(&api_at(&server.uri()));

        for (label, outcome) in clients.every_call().await {
            if !reads_a_body.contains(&label) {
                assert!(outcome.is_ok(), "{label}: {outcome:?}");
            }
        }
    }

    #[tokio::test]
    async fn status_failures_map_to_typed_platform_errors() {
        type Check = fn(&PlatformError) -> bool;
        let cases: [(ResponseTemplate, &str, Check); 6] = [
            (
                ResponseTemplate::new(429).insert_header("retry-after", "42"),
                "429 with Retry-After",
                |e| {
                    matches!(
                        e,
                        PlatformError::RateLimited {
                            retry_after_secs: 42
                        }
                    )
                },
            ),
            (
                ResponseTemplate::new(429),
                "429 without Retry-After",
                |e| matches!(e, PlatformError::RateLimited { retry_after_secs } if *retry_after_secs == forge_net::DEFAULT_RETRY_AFTER_SECS),
            ),
            (
                ResponseTemplate::new(503).insert_header("retry-after", "7"),
                "503 with Retry-After",
                |e| {
                    matches!(
                        e,
                        PlatformError::RateLimited {
                            retry_after_secs: 7
                        }
                    )
                },
            ),
            (
                ResponseTemplate::new(503).set_body_string("down"),
                "503 without Retry-After",
                |e| matches!(e, PlatformError::Http { status: 503, body } if body == "down"),
            ),
            (
                ResponseTemplate::new(422).set_body_string("bad title"),
                "422",
                |e| matches!(e, PlatformError::Http { status: 422, body } if body == "bad title"),
            ),
            (ResponseTemplate::new(404), "404", |e| {
                matches!(e, PlatformError::Http { status: 404, .. })
            }),
        ];

        for (template, what, check) in cases {
            let server = answering(template).await;
            let clients = Clients::over(&api_at(&server.uri()));

            for (label, outcome) in clients.every_call().await {
                let err = outcome.unwrap_err();
                assert!(check(&err), "{label} on {what}: {err:?}");
            }
        }
    }

    #[tokio::test]
    async fn unauthorized_is_auth_for_every_client() {
        let server = answering(ResponseTemplate::new(401)).await;
        let clients = Clients::over(&api_at(&server.uri()));

        for (label, outcome) in clients.every_call().await {
            assert!(
                matches!(outcome, Err(PlatformError::Auth { .. })),
                "{label}: {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn forbidden_is_auth_with_a_scope_hint_except_for_send_and_categories() {
        let server = answering(ResponseTemplate::new(403).set_body_string("nope")).await;
        let clients = Clients::over(&api_at(&server.uri()));
        let scope_hints = [
            ("delete chat", "moderation:chat_message:manage"),
            ("channel update", "channel scope"),
            ("channel get", "channel scope"),
            ("ban", "moderation:ban"),
            ("unban", "moderation:ban"),
            ("reward create", "channel:rewards:write"),
            ("pending redemptions", "channel:rewards:write"),
        ];

        for (label, outcome) in clients.every_call().await {
            let err = outcome.unwrap_err();
            match scope_hints.iter().find(|(hinted, _)| *hinted == label) {
                Some((_, hint)) => assert!(
                    matches!(&err, PlatformError::Auth { reason } if reason.contains(hint)),
                    "{label}: {err:?}"
                ),
                None => assert!(
                    matches!(&err, PlatformError::Http { status: 403, body } if body == "nope"),
                    "{label}: {err:?}"
                ),
            }
        }
    }

    #[tokio::test]
    async fn an_exhausted_budget_fails_every_client_without_a_request() {
        let server = answering(ResponseTemplate::new(200)).await;
        let clients = Clients::over(&api_limited_by(&server.uri(), Arc::new(ExhaustedLimiter)));

        for (label, outcome) in clients.every_call().await {
            assert!(
                matches!(outcome, Err(PlatformError::RateLimitExhausted)),
                "{label}: {outcome:?}"
            );
        }
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn every_client_draws_from_the_one_official_budget() {
        let server =
            answering(ResponseTemplate::new(200).set_body_string(r#"{"data":[{}]}"#)).await;
        let calls = Clients::over(&api_at(&server.uri()))
            .every_call()
            .await
            .len();
        let shared = Clients::over(&api_limited_by(
            &server.uri(),
            Arc::new(GrantsThenExhausts::granting(calls)),
        ));

        let first_round = shared.every_call().await;
        let second_round = shared.every_call().await;

        assert!(first_round.iter().all(|(_, outcome)| outcome.is_ok()));
        for (label, outcome) in second_round {
            assert!(
                matches!(outcome, Err(PlatformError::RateLimitExhausted)),
                "{label}: {outcome:?}"
            );
        }
        assert_eq!(request_count(&server).await, calls * 2);
    }

    #[tokio::test]
    async fn a_throttle_seen_by_one_client_cools_every_official_client() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "60"))
            .mount(&server)
            .await;
        let clients = Clients::over(&api_limited_by(&server.uri(), official_bucket()));
        let throttled = clients.send.send("hi", TOKEN, 42, false).await;
        tokio::time::pause();

        let after = clients.every_call().await;

        assert!(
            matches!(
                throttled,
                Err(PlatformError::RateLimited {
                    retry_after_secs: 60
                })
            ),
            "{throttled:?}"
        );
        for (label, outcome) in after {
            assert!(
                matches!(outcome, Err(PlatformError::RateLimitExhausted)),
                "{label}: {outcome:?}"
            );
        }
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn a_channel_info_refusal_leaves_official_calls_unaffected() {
        for (refusal, what) in [
            (
                ResponseTemplate::new(429).insert_header("retry-after", "600"),
                "429",
            ),
            (
                ResponseTemplate::new(403).set_body_string("<html>Just a moment...</html>"),
                "cloudflare 403",
            ),
        ] {
            let channel_api = answering(refusal).await;
            let official = answering(ResponseTemplate::new(200)).await;
            let bus = Arc::new(RecordingPublisher::default());
            let fetcher = channel_info_with(&channel_api.uri(), channel_info_bucket(), bus.clone());
            let send = KickSendChat::new(api_with(&official.uri(), official_bucket(), bus));

            let refused = fetcher.fetch("streamer").await;
            let sent = send.send("hi", TOKEN, 42, false).await;

            assert!(
                matches!(refused, Err(KickError::Http { .. })),
                "{what}: {refused:?}"
            );
            assert!(sent.is_ok(), "{what}: {sent:?}");
            assert_eq!(request_count(&official).await, 1, "{what}");
        }
    }

    #[tokio::test]
    async fn a_status_failure_publishes_request_fail_with_the_endpoint_template() {
        for (template, expected) in [
            (
                ResponseTemplate::new(500).set_body_string("boom"),
                serde_json::json!({
                    "endpoint": "/chat/{message_id}",
                    "status_code": 500,
                    "body_snippet": "boom",
                    "retry_after_secs": null,
                }),
            ),
            (
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "42")
                    .set_body_string("slow down"),
                serde_json::json!({
                    "endpoint": "/chat/{message_id}",
                    "status_code": 429,
                    "body_snippet": "slow down",
                    "retry_after_secs": 42,
                }),
            ),
        ] {
            let server = answering(template).await;
            let bus = Arc::new(RecordingPublisher::default());
            let send = KickSendChat::new(api_with(
                &server.uri(),
                Arc::new(crate::endpoints_test_support::GrantingLimiter),
                bus.clone(),
            ));

            send.delete("msg-77", TOKEN).await.unwrap_err();

            let published: Vec<(EventSource, serde_json::Value)> = bus
                .events_of_kind(RequestFailed::KIND)
                .into_iter()
                .map(|event| (event.source, event.payload))
                .collect();
            assert_eq!(published, vec![(EventSource::Kick, expected)]);
        }
    }

    #[tokio::test]
    async fn every_refusal_status_publishes_request_fail_for_every_client() {
        for status in [401_u16, 403, 404, 422] {
            let server = answering(ResponseTemplate::new(status)).await;
            let bus = Arc::new(RecordingPublisher::default());
            let clients = Clients::over(&api_with(
                &server.uri(),
                Arc::new(crate::endpoints_test_support::GrantingLimiter),
                bus.clone(),
            ));

            let calls = clients.every_call().await.len();

            assert_eq!(
                bus.events_of_kind(RequestFailed::KIND).len(),
                calls,
                "status {status}"
            );
        }
    }

    #[tokio::test]
    async fn failures_without_a_status_publish_no_request_fail() {
        let bus = Arc::new(RecordingPublisher::default());
        let unreachable = KickSendChat::new(api_with(
            "http://127.0.0.1:0",
            Arc::new(crate::endpoints_test_support::GrantingLimiter),
            bus.clone(),
        ));
        let exhausted = KickSendChat::new(api_with(
            "http://127.0.0.1:0",
            Arc::new(ExhaustedLimiter),
            bus.clone(),
        ));
        let server = answering(ResponseTemplate::new(200)).await;
        let succeeding = KickSendChat::new(api_with(
            &server.uri(),
            Arc::new(crate::endpoints_test_support::GrantingLimiter),
            bus.clone(),
        ));

        let outcomes = [
            unreachable.send("hi", TOKEN, 42, false).await.is_err(),
            exhausted.send("hi", TOKEN, 42, false).await.is_err(),
            succeeding.send("hi", TOKEN, 42, false).await.is_ok(),
        ];

        assert_eq!(outcomes, [true, true, true]);
        assert!(bus.events_of_kind(RequestFailed::KIND).is_empty());
    }

    #[tokio::test]
    async fn request_fail_body_snippet_is_capped_at_its_char_limit() {
        for (body_chars, expected_chars) in [
            (BODY_SNIPPET_MAX_CHARS - 1, BODY_SNIPPET_MAX_CHARS - 1),
            (BODY_SNIPPET_MAX_CHARS, BODY_SNIPPET_MAX_CHARS),
            (BODY_SNIPPET_MAX_CHARS + 1, BODY_SNIPPET_MAX_CHARS),
        ] {
            let server =
                answering(ResponseTemplate::new(500).set_body_string("\u{456}".repeat(body_chars)))
                    .await;
            let bus = Arc::new(RecordingPublisher::default());
            let send = KickSendChat::new(api_with(
                &server.uri(),
                Arc::new(crate::endpoints_test_support::GrantingLimiter),
                bus.clone(),
            ));

            send.send("hi", TOKEN, 42, false).await.unwrap_err();

            let events = bus.events_of_kind(RequestFailed::KIND);
            let snippet = events[0].payload["body_snippet"].as_str().unwrap();
            assert_eq!(
                snippet.chars().count(),
                expected_chars,
                "{body_chars} chars"
            );
        }
    }

    #[tokio::test]
    async fn an_echoed_token_never_reaches_errors_or_request_fail() {
        let echo = format!("invalid token {TOKEN} for Bearer {TOKEN}");
        for status in [401_u16, 403, 429, 500] {
            let server =
                answering(ResponseTemplate::new(status).set_body_string(echo.clone())).await;
            let bus = Arc::new(RecordingPublisher::default());
            let clients = Clients::over(&api_with(
                &server.uri(),
                Arc::new(crate::endpoints_test_support::GrantingLimiter),
                bus.clone(),
            ));

            for (label, outcome) in clients.every_call().await {
                let err = outcome.unwrap_err();
                assert!(
                    !format!("{err} {err:?}").contains(TOKEN),
                    "{label} on {status} leaked the token: {err:?}"
                );
            }
            for event in bus.events_of_kind(RequestFailed::KIND) {
                assert!(
                    !event.payload.to_string().contains(TOKEN),
                    "request.fail on {status} leaked the token: {}",
                    event.payload
                );
            }
        }
    }

    #[tokio::test]
    async fn a_transport_failure_never_carries_the_token_or_the_url() {
        let clients = Clients::over(&api_at("http://127.0.0.1:0"));

        for (label, outcome) in clients.every_call().await {
            let err = outcome.unwrap_err();
            let rendered = format!("{err} {err:?}");
            assert!(
                matches!(err, PlatformError::Network { .. }) && !rendered.contains(TOKEN),
                "{label}: {rendered}"
            );
        }
    }
}
