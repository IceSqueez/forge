use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource};
use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FailureContext, FailureObserver, Gateway,
    GatewayBuildError, GatewayFailure, GatewayProfile, GatewayRequest, GatewayResponse,
    RateLimiter, RequestBody, ThrottleHintFamily, TokenBucketRateLimiter,
};
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};

use crate::quota_state::{SharedQuota, today_pacific};

const YOUTUBE_CALL_BUDGET_CAPACITY: u32 = 10;
const YOUTUBE_CALL_BUDGET_WINDOW: Duration = Duration::from_secs(10);

const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);
const REQUEST_FAIL_KIND: &str = "request.fail";
const BODY_SNIPPET_MAX_CHARS: usize = 200;
const JSON_CONTENT_TYPE: &str = "application/json";
const QUOTA_EXCEEDED_REASON: &str = "quotaExceeded";
const SCOPE_MISSING_REASONS: [&str; 2] = ["insufficientPermissions", "operationNotSupported"];

const YOUTUBE_DATA_API_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

const YOUTUBE_UPLOAD_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Media,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

#[derive(Clone)]
pub struct YoutubeDataApi {
    data: Gateway,
    upload: Gateway,
    quota: SharedQuota,
}

impl YoutubeDataApi {
    pub fn new(
        endpoints: &PlatformEndpoints,
        quota: SharedQuota,
        bus: Arc<dyn EventPublisher>,
    ) -> Result<Self, GatewayBuildError> {
        let observer: Arc<dyn FailureObserver> = Arc::new(RequestFailPublisher { bus });
        let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            YOUTUBE_CALL_BUDGET_CAPACITY,
            YOUTUBE_CALL_BUDGET_WINDOW,
        ));
        let data = Gateway::new(
            endpoints.base_url(EndpointSurface::YouTubeDataApi),
            Arc::clone(&limiter),
            YOUTUBE_DATA_API_PROFILE,
            Some(Arc::clone(&observer)),
        )?;
        let upload = Gateway::new(
            endpoints.base_url(EndpointSurface::YouTubeUploadApi),
            limiter,
            YOUTUBE_UPLOAD_PROFILE,
            Some(observer),
        )?;
        Ok(Self {
            data,
            upload,
            quota,
        })
    }

    pub fn quota(&self) -> &SharedQuota {
        &self.quota
    }

    pub(crate) async fn send(
        &self,
        cost: u32,
        request: GatewayRequest,
    ) -> Result<GatewayResponse, DataApiError> {
        self.charged_send(&self.data, cost, request).await
    }

    pub(crate) async fn upload(
        &self,
        cost: u32,
        request: GatewayRequest,
    ) -> Result<GatewayResponse, DataApiError> {
        self.charged_send(&self.upload, cost, request).await
    }

    async fn charged_send(
        &self,
        gateway: &Gateway,
        cost: u32,
        request: GatewayRequest,
    ) -> Result<GatewayResponse, DataApiError> {
        let day = today_pacific();
        self.quota
            .lock()
            .await
            .charge(cost, day)
            .map_err(|_| DataApiError::QuotaExhausted)?;
        let outcome = gateway.send(request).await;
        if let Err(failure) = &outcome
            && never_reached_upstream(failure)
        {
            self.quota.lock().await.refund(cost, day);
        }
        outcome.map_err(DataApiError::Gateway)
    }
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
        self.bus.publish(Event::new(
            EventSource::YouTube,
            REQUEST_FAIL_KIND,
            serde_json::json!({
                "endpoint": context.label.unwrap_or_default(),
                "status_code": status,
                "body_snippet": body_snippet,
                "retry_after_secs": retry_after_secs,
            }),
        ));
    }
}

fn never_reached_upstream(failure: &GatewayFailure) -> bool {
    matches!(
        failure,
        GatewayFailure::BudgetExhausted { .. }
            | GatewayFailure::Cancelled
            | GatewayFailure::InvalidRequest(_)
    )
}

#[derive(Debug)]
pub(crate) enum DataApiError {
    QuotaExhausted,
    Gateway(GatewayFailure),
}

impl DataApiError {
    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Gateway(GatewayFailure::Cancelled))
    }

    pub(crate) fn forbidden_with_reason(&self, reason: &str) -> bool {
        match self {
            Self::Gateway(GatewayFailure::Forbidden { excerpt }) => {
                excerpt.as_str().contains(reason)
            }
            _ => false,
        }
    }

    pub(crate) fn into_platform_error(self, missing_scope: &str) -> PlatformError {
        let failure = match self {
            Self::QuotaExhausted => return PlatformError::QuotaExhausted,
            Self::Gateway(failure) => failure,
        };
        match failure {
            GatewayFailure::RateLimited { retry_after, .. } => PlatformError::RateLimited {
                retry_after_secs: u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX),
            },
            GatewayFailure::BudgetExhausted { .. } => PlatformError::RateLimitExhausted,
            GatewayFailure::Forbidden { excerpt }
                if excerpt.as_str().contains(QUOTA_EXCEEDED_REASON) =>
            {
                PlatformError::QuotaExhausted
            }
            GatewayFailure::Forbidden { excerpt }
                if SCOPE_MISSING_REASONS
                    .iter()
                    .any(|reason| excerpt.as_str().contains(reason)) =>
            {
                PlatformError::Auth {
                    reason: missing_scope.to_owned(),
                }
            }
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
}

pub(crate) fn json_body(value: &serde_json::Value) -> Result<RequestBody, PlatformError> {
    Ok(RequestBody::raw(
        JSON_CONTENT_TYPE,
        serde_json::to_vec(value)?,
    ))
}

pub(crate) fn malformed(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::MalformedResponse {
        reason: error.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use forge_events::EventSource;
    use forge_net::{GatewayRequest, Secret};
    use forge_platform_core::{FollowStatus, PlatformError};
    use futures::future::BoxFuture;
    use reqwest::StatusCode;
    use serde_json::{Value, json};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::ad_break::YoutubeAdBreak;
    use crate::ban_ledger_test_support::{MemoryBanLedger, broadcaster_source};
    use crate::channel_lookup::YoutubeChannelLookup;
    use crate::endpoints_test_support::{
        RecordingPublisher, StubReply, StubServer, data_api_charging, data_api_with, exhaust_quota,
        used_today,
    };
    use crate::follow_lookup::YoutubeFollowLookup;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::moderation::YoutubeModeration;
    use crate::send_chat::YoutubeSendChat;
    use crate::stream_metadata::YoutubeStreamMetadata;
    use crate::stream_stats::YoutubeStreamStats;
    use crate::thumbnail::YoutubeThumbnail;

    type TokenSource =
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

    const TOKEN: &str = "ya29.secret-access-token";
    const COST: u32 = 7;
    const PATH: &str = "/videos";
    const SCOPE: &str = "test scope missing";
    const NO_HINT_RETRY_AFTER_SECS: u32 = 30;
    const LONG_RETRY_AFTER: &str = "3600";
    const VIEWER: &str = "UCviewerchannelid0000042";

    fn request() -> GatewayRequest {
        GatewayRequest::get(PATH)
            .label(PATH)
            .bearer(Secret::new(TOKEN))
    }

    fn refusal(status: StatusCode, reason: &str) -> StubReply {
        StubReply::json(
            status.as_u16(),
            &json!({ "error": { "code": status.as_u16(), "errors": [{ "reason": reason }] } }),
        )
    }

    fn throttled(retry_after: Option<&str>) -> StubReply {
        let reply = StubReply::status(StatusCode::TOO_MANY_REQUESTS.as_u16());
        match retry_after {
            Some(value) => reply.header("retry-after", value),
            None => reply,
        }
    }

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) }))
    }

    fn refusing_token_source() -> TokenSource {
        Arc::new(|| {
            Box::pin(async {
                Err(PlatformError::ReauthRequired {
                    platform: "youtube".to_owned(),
                })
            })
        })
    }

    async fn sent_through(reply: StubReply) -> (Result<GatewayResponse, DataApiError>, u32) {
        let server = StubServer::start().await;
        server.reply_always(PATH, reply);
        let quota = SharedQuota::default();
        let api = data_api_charging(&server.uri(), quota.clone());
        let outcome = api.send(COST, request()).await;
        (outcome, used_today(&quota).await)
    }

    async fn platform_error_for(reply: StubReply) -> PlatformError {
        sent_through(reply)
            .await
            .0
            .expect_err("the call must fail")
            .into_platform_error(SCOPE)
    }

    async fn request_fails_published_for(reply: StubReply) -> Vec<Event> {
        let server = StubServer::start().await;
        server.reply_always(PATH, reply);
        let bus = Arc::new(RecordingPublisher::default());
        let api = data_api_with(&server.uri(), SharedQuota::default(), bus.clone());
        api.send(COST, request()).await.ok();
        bus.events_of_kind(REQUEST_FAIL_KIND)
    }

    #[tokio::test]
    async fn a_request_that_reached_google_stays_charged_whatever_the_answer() {
        for (label, reply) in [
            ("ok", StubReply::ok(&json!({}))),
            (
                "server error",
                StubReply::status(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
            ),
            (
                "quota refusal",
                refusal(StatusCode::FORBIDDEN, QUOTA_EXCEEDED_REASON),
            ),
            (
                "not found",
                StubReply::status(StatusCode::NOT_FOUND.as_u16()),
            ),
            ("throttled", throttled(Some("5"))),
        ] {
            assert_eq!(sent_through(reply).await.1, COST, "{label}");
        }
    }

    #[tokio::test]
    async fn a_transport_failure_stays_charged() {
        let quota = SharedQuota::default();
        let api = data_api_charging("http://127.0.0.1:1", quota.clone());

        let outcome = api.send(COST, request()).await;

        assert!(
            matches!(
                outcome,
                Err(DataApiError::Gateway(GatewayFailure::Transport(_)))
            ),
            "{outcome:?}"
        );
        assert_eq!(used_today(&quota).await, COST);
    }

    #[tokio::test]
    async fn requests_that_never_left_are_refunded() {
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        for (label, request) in [
            ("cancelled", request().cancel_on(cancelled)),
            ("invalid", GatewayRequest::get("/videos?id=x").label(PATH)),
        ] {
            let server = StubServer::start().await;
            server.reply_always(PATH, StubReply::ok(&json!({})));
            let quota = SharedQuota::default();
            let api = data_api_charging(&server.uri(), quota.clone());

            let outcome = api.send(COST, request).await;

            assert!(outcome.is_err(), "{label}");
            assert_eq!(
                (used_today(&quota).await, server.request_count(PATH)),
                (0, 0),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn a_spent_daily_quota_refuses_without_a_request_or_an_event() {
        let server = StubServer::start().await;
        server.reply_always(PATH, StubReply::ok(&json!({})));
        let quota = SharedQuota::default();
        exhaust_quota(&quota).await;
        let bus = Arc::new(RecordingPublisher::default());
        let api = data_api_with(&server.uri(), quota, bus.clone());

        let outcome = api.send(COST, request()).await;

        assert!(matches!(outcome, Err(DataApiError::QuotaExhausted)));
        assert_eq!(
            (
                server.request_count(PATH),
                bus.events_of_kind(REQUEST_FAIL_KIND).len()
            ),
            (0, 0)
        );
    }

    enum Mapped {
        RateLimited(u32),
        Quota,
        MissingScope,
        Http(StatusCode, &'static str),
    }

    fn maps_to(error: &PlatformError, expected: &Mapped) -> bool {
        match (error, expected) {
            (PlatformError::RateLimited { retry_after_secs }, Mapped::RateLimited(secs)) => {
                retry_after_secs == secs
            }
            (PlatformError::QuotaExhausted, Mapped::Quota) => true,
            (PlatformError::Auth { reason }, Mapped::MissingScope) => reason == SCOPE,
            (PlatformError::Http { status, body }, Mapped::Http(expected, needle)) => {
                *status == expected.as_u16() && body.contains(needle)
            }
            _ => false,
        }
    }

    #[tokio::test]
    async fn every_status_failure_maps_to_its_platform_error() {
        let unavailable = StubReply::status(StatusCode::SERVICE_UNAVAILABLE.as_u16());
        for (label, reply, expected) in [
            (
                "429 with Retry-After",
                throttled(Some("45")),
                Mapped::RateLimited(45),
            ),
            (
                "429 without a hint",
                throttled(None),
                Mapped::RateLimited(NO_HINT_RETRY_AFTER_SECS),
            ),
            (
                "503 with Retry-After",
                unavailable.clone().header("retry-after", "7"),
                Mapped::RateLimited(7),
            ),
            (
                "503 without a hint",
                unavailable,
                Mapped::Http(StatusCode::SERVICE_UNAVAILABLE, ""),
            ),
            (
                "403 quotaExceeded",
                refusal(StatusCode::FORBIDDEN, QUOTA_EXCEEDED_REASON),
                Mapped::Quota,
            ),
            (
                "403 insufficientPermissions",
                refusal(StatusCode::FORBIDDEN, "insufficientPermissions"),
                Mapped::MissingScope,
            ),
            (
                "403 operationNotSupported",
                refusal(StatusCode::FORBIDDEN, "operationNotSupported"),
                Mapped::MissingScope,
            ),
            (
                "403 other reason",
                refusal(StatusCode::FORBIDDEN, "forbidden"),
                Mapped::Http(StatusCode::FORBIDDEN, "forbidden"),
            ),
            (
                "401",
                refusal(StatusCode::UNAUTHORIZED, "authError"),
                Mapped::Http(StatusCode::UNAUTHORIZED, "authError"),
            ),
            (
                "404",
                StubReply::status(StatusCode::NOT_FOUND.as_u16()),
                Mapped::Http(StatusCode::NOT_FOUND, ""),
            ),
            (
                "400",
                refusal(StatusCode::BAD_REQUEST, "invalidValue"),
                Mapped::Http(StatusCode::BAD_REQUEST, "invalidValue"),
            ),
            (
                "500",
                StubReply::status(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
                Mapped::Http(StatusCode::INTERNAL_SERVER_ERROR, ""),
            ),
        ] {
            let error = platform_error_for(reply).await;
            assert!(maps_to(&error, &expected), "{label}: {error:?}");
        }
    }

    #[tokio::test]
    async fn a_transport_failure_maps_to_network_without_the_url_or_token() {
        let api = data_api_charging("http://127.0.0.1:1", SharedQuota::default());

        let error = api
            .send(COST, request())
            .await
            .unwrap_err()
            .into_platform_error(SCOPE);

        let PlatformError::Network { reason } = &error else {
            panic!("expected Network, got {error:?}");
        };
        assert!(
            !reason.contains("127.0.0.1") && !reason.contains(TOKEN) && !reason.contains(PATH),
            "{reason}"
        );
    }

    #[tokio::test]
    async fn an_echoed_bearer_token_never_reaches_the_error_or_the_event() {
        let echo = format!("{{\"error\":{{\"message\":\"bad header Bearer {TOKEN}\"}}}}");
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let reply = StubReply::json(status.as_u16(), &serde_json::from_str(&echo).unwrap());
            let error = platform_error_for(reply.clone()).await;
            let events = request_fails_published_for(reply).await;
            let mut surfaces = vec![error.to_string(), format!("{error:?}")];
            surfaces.extend(events.iter().map(|event| event.payload.to_string()));

            for surface in surfaces {
                assert!(!surface.contains(TOKEN), "{status}: {surface}");
            }
        }
    }

    #[tokio::test]
    async fn a_status_failure_publishes_request_fail_with_the_twitch_payload_keys() {
        let events =
            request_fails_published_for(refusal(StatusCode::INTERNAL_SERVER_ERROR, "backendError"))
                .await;

        let [event] = events.as_slice() else {
            panic!("expected one request.fail, got {events:?}");
        };
        let snippet = event.payload["body_snippet"].as_str().unwrap();
        assert_eq!(
            (
                event.source,
                event.payload["endpoint"].clone(),
                event.payload["status_code"].clone(),
                event.payload["retry_after_secs"].clone(),
                snippet.contains("backendError"),
            ),
            (
                EventSource::YouTube,
                json!(PATH),
                json!(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
                Value::Null,
                true
            )
        );
    }

    #[tokio::test]
    async fn request_fail_carries_the_status_and_wait_of_each_refusal() {
        for (label, reply, status, retry_after) in [
            (
                "quota refusal",
                refusal(StatusCode::FORBIDDEN, QUOTA_EXCEEDED_REASON),
                StatusCode::FORBIDDEN,
                Value::Null,
            ),
            (
                "throttle with hint",
                throttled(Some("45")),
                StatusCode::TOO_MANY_REQUESTS,
                json!(45),
            ),
            (
                "throttle without hint",
                throttled(None),
                StatusCode::TOO_MANY_REQUESTS,
                json!(NO_HINT_RETRY_AFTER_SECS),
            ),
            (
                "not found",
                StubReply::status(StatusCode::NOT_FOUND.as_u16()),
                StatusCode::NOT_FOUND,
                Value::Null,
            ),
        ] {
            let events = request_fails_published_for(reply).await;
            let published: Vec<(Value, Value)> = events
                .iter()
                .map(|event| {
                    (
                        event.payload["status_code"].clone(),
                        event.payload["retry_after_secs"].clone(),
                    )
                })
                .collect();
            assert_eq!(
                published,
                [(json!(status.as_u16()), retry_after)],
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn request_fail_body_snippet_keeps_at_most_two_hundred_characters() {
        for (body_chars, kept) in [
            (BODY_SNIPPET_MAX_CHARS - 1, BODY_SNIPPET_MAX_CHARS - 1),
            (BODY_SNIPPET_MAX_CHARS, BODY_SNIPPET_MAX_CHARS),
            (BODY_SNIPPET_MAX_CHARS + 1, BODY_SNIPPET_MAX_CHARS),
        ] {
            let reply = StubReply::text(
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                &"\u{e9}".repeat(body_chars),
            );

            let events = request_fails_published_for(reply).await;

            let snippet = events[0].payload["body_snippet"].as_str().unwrap();
            assert_eq!(snippet.chars().count(), kept, "{body_chars}");
        }
    }

    #[tokio::test]
    async fn failures_that_never_got_an_answer_publish_nothing() {
        let unreachable_bus = Arc::new(RecordingPublisher::default());
        data_api_with(
            "http://127.0.0.1:1",
            SharedQuota::default(),
            unreachable_bus.clone(),
        )
        .send(COST, request())
        .await
        .ok();

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let server = StubServer::start().await;
        server.reply_always(PATH, StubReply::ok(&json!({})));
        let cancel_bus = Arc::new(RecordingPublisher::default());
        data_api_with(&server.uri(), SharedQuota::default(), cancel_bus.clone())
            .send(COST, request().cancel_on(cancelled))
            .await
            .ok();

        assert_eq!(
            (
                unreachable_bus.events_of_kind(REQUEST_FAIL_KIND).len(),
                cancel_bus.events_of_kind(REQUEST_FAIL_KIND).len()
            ),
            (0, 0)
        );
    }

    struct Clients {
        stats: YoutubeStreamStats,
        ad_break: YoutubeAdBreak,
        thumbnail: YoutubeThumbnail,
        lookup: YoutubeChannelLookup,
        metadata: YoutubeStreamMetadata,
        moderation: YoutubeModeration,
        send: YoutubeSendChat,
        follow: YoutubeFollowLookup,
    }

    fn clients(api: &YoutubeDataApi, token: &TokenSource) -> Clients {
        let broadcast = ActiveBroadcastIdHandle::new();
        broadcast.set(Some("bc-live".to_owned()));
        let live = LiveChatIdHandle::new();
        live.set(Some("lc-live".to_owned()));
        Clients {
            stats: YoutubeStreamStats::new(api.clone(), token.clone(), broadcast.clone()),
            ad_break: YoutubeAdBreak::new(api.clone(), token.clone(), broadcast.clone()),
            thumbnail: YoutubeThumbnail::new(api.clone(), token.clone(), broadcast.clone()),
            lookup: YoutubeChannelLookup::new(api.clone(), token.clone()),
            metadata: YoutubeStreamMetadata::new(api.clone(), token.clone(), broadcast),
            moderation: YoutubeModeration::new(
                api.clone(),
                token.clone(),
                broadcaster_source(),
                live.clone(),
                MemoryBanLedger::shared(),
            ),
            send: YoutubeSendChat::new(api.clone(), token.clone(), live),
            follow: YoutubeFollowLookup::new(
                api.clone(),
                token.clone(),
                "UCbroadcasterchannel0001".to_owned(),
            ),
        }
    }

    async fn every_client_outcome(
        clients: &Clients,
        image: &str,
    ) -> Vec<(&'static str, Result<(), PlatformError>)> {
        vec![
            ("stream stats", clients.stats.fetch().await.map(drop)),
            ("ad break", clients.ad_break.insert_cuepoint(30).await),
            ("thumbnail upload", clients.thumbnail.set(image).await),
            (
                "channel lookup",
                clients.lookup.lookup("@viewer").await.map(drop),
            ),
            ("metadata update", clients.metadata.set_title("t").await),
            (
                "moderator add",
                clients.moderation.add_moderator(VIEWER).await,
            ),
            ("chat send", clients.send.send("hi").await),
        ]
    }

    #[tokio::test]
    async fn a_throttle_on_one_client_pauses_every_client_of_the_credential() {
        let server = StubServer::start().await;
        server.reply_always("/liveChat/messages", throttled(Some(LONG_RETRY_AFTER)));
        let quota = SharedQuota::default();
        let api = data_api_charging(&server.uri(), quota.clone());
        let clients = clients(&api, &token_source());
        let image = tempfile::NamedTempFile::new().unwrap();
        let first = clients.send.send("hi").await;
        let charged_by_throttled_call = used_today(&quota).await;
        tokio::time::pause();

        let outcomes = every_client_outcome(&clients, image.path().to_str().unwrap()).await;
        let follow = clients.follow.status_of(VIEWER).await;

        assert!(
            matches!(first, Err(PlatformError::RateLimited { .. })),
            "{first:?}"
        );
        for (label, outcome) in outcomes {
            assert!(
                matches!(outcome, Err(PlatformError::RateLimitExhausted)),
                "{label}: {outcome:?}"
            );
        }
        assert_eq!(
            (
                follow,
                server.request_count("/liveChat/messages"),
                used_today(&quota).await
            ),
            (FollowStatus::Unavailable, 1, charged_by_throttled_call)
        );
    }

    #[tokio::test]
    async fn a_refused_access_token_charges_nothing_and_sends_nothing() {
        let server = StubServer::start().await;
        let quota = SharedQuota::default();
        let api = data_api_charging(&server.uri(), quota.clone());
        let clients = clients(&api, &refusing_token_source());
        let image = tempfile::NamedTempFile::new().unwrap();

        let outcomes = every_client_outcome(&clients, image.path().to_str().unwrap()).await;
        let follow = clients.follow.status_of(VIEWER).await;

        for (label, outcome) in outcomes {
            assert!(
                matches!(outcome, Err(PlatformError::ReauthRequired { .. })),
                "{label}: {outcome:?}"
            );
        }
        let answered: usize = ["/videos", "/channels", "/liveChat/messages"]
            .iter()
            .map(|path| server.request_count(path))
            .sum();
        assert_eq!(
            (follow, answered, used_today(&quota).await),
            (FollowStatus::Unavailable, 0, 0)
        );
    }
}
