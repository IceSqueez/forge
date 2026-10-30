use std::sync::Arc;

use async_trait::async_trait;
pub(crate) use forge_net::status::CONFLICT;
use forge_net::status::TOO_MANY_REQUESTS;
use forge_net::{ExpectedRefusal, Gateway, GatewayFailure, GatewayRequest, Method, RequestBody};
use forge_types::OAuthToken;
use forge_types::Secret;
use thiserror::Error;
use tracing::{debug, warn};

pub(crate) const CLIENT_ID_HEADER: &str = "Client-Id";
const SERVER_ERROR_STATUSES: std::ops::RangeInclusive<u16> = 500..=599;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelixMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

impl From<HelixMethod> for Method {
    fn from(method: HelixMethod) -> Self {
        match method {
            HelixMethod::Get => Self::Get,
            HelixMethod::Post => Self::Post,
            HelixMethod::Patch => Self::Patch,
            HelixMethod::Put => Self::Put,
            HelixMethod::Delete => Self::Delete,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HelixRequest {
    pub method: HelixMethod,
    pub path: &'static str,
    pub query: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
    pub expected_refusals: Vec<ExpectedRefusal>,
}

impl HelixRequest {
    pub fn new(method: HelixMethod, path: &'static str) -> Self {
        Self {
            method,
            path,
            query: Vec::new(),
            body: None,
            expected_refusals: Vec::new(),
        }
    }

    pub fn query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body);
        self
    }

    pub fn expect_status(mut self, status: u16) -> Self {
        self.expected_refusals.push(ExpectedRefusal::status(status));
        self
    }

    pub fn expect_status_with_excerpt(mut self, status: u16, marker: &'static str) -> Self {
        self.expected_refusals
            .push(ExpectedRefusal::status_with_excerpt(status, marker));
        self
    }
}

#[derive(Debug, Error)]
pub enum HelixError {
    #[error("rate limited")]
    RateLimited,
    #[error("reauth required")]
    ReauthRequired,
    #[error("{0}")]
    Credentials(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("{0}")]
    Transport(String),
    #[error("{0}")]
    UndecodableResponse(String),
}

impl HelixError {
    pub(crate) fn is_already_exists(&self) -> bool {
        matches!(self, Self::Http { status, .. } if *status == CONFLICT)
    }

    pub(crate) fn is_server_error(&self) -> bool {
        matches!(self, Self::Http { status, .. } if SERVER_ERROR_STATUSES.contains(status))
    }
}

impl From<GatewayFailure> for HelixError {
    fn from(failure: GatewayFailure) -> Self {
        match failure {
            GatewayFailure::Unauthorized { .. } => Self::ReauthRequired,
            GatewayFailure::BudgetExhausted { .. } => Self::RateLimited,
            GatewayFailure::RateLimited { status, .. } if status == TOO_MANY_REQUESTS => {
                Self::RateLimited
            }
            other => match other.status() {
                Some(status) => Self::Http {
                    status,
                    body: other
                        .excerpt()
                        .map(|excerpt| excerpt.as_str().to_owned())
                        .unwrap_or_default(),
                },
                None => Self::Transport(other.to_string()),
            },
        }
    }
}

#[async_trait]
pub trait HelixTokenSource: Send + Sync {
    async fn access_token(&self) -> Result<OAuthToken, HelixError>;
}

#[async_trait]
pub trait HelixTokenRefresher: Send + Sync {
    async fn refresh(&self, failed_token: &OAuthToken) -> Result<OAuthToken, HelixError>;
}

pub(crate) struct FixedTokenSource(OAuthToken);

impl FixedTokenSource {
    pub(crate) fn new(token: OAuthToken) -> Self {
        Self(token)
    }
}

#[async_trait]
impl HelixTokenSource for FixedTokenSource {
    async fn access_token(&self) -> Result<OAuthToken, HelixError> {
        Ok(self.0.clone())
    }
}

#[async_trait]
pub trait HelixTransport: Send + Sync {
    async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError>;
}

pub struct HelixHttpTransport {
    gateway: Gateway,
    client_id: String,
    tokens: Arc<dyn HelixTokenSource>,
    refresher: Option<Arc<dyn HelixTokenRefresher>>,
}

impl HelixHttpTransport {
    pub fn new(gateway: Gateway, client_id: String, tokens: Arc<dyn HelixTokenSource>) -> Self {
        Self {
            gateway,
            client_id,
            tokens,
            refresher: None,
        }
    }

    pub fn with_refresher(mut self, refresher: Arc<dyn HelixTokenRefresher>) -> Self {
        self.refresher = Some(refresher);
        self
    }

    fn outbound(
        &self,
        request: &HelixRequest,
        token: &OAuthToken,
    ) -> Result<GatewayRequest, HelixError> {
        let mut outbound = GatewayRequest::new(request.method.into(), request.path)
            .label(request.path)
            .header(CLIENT_ID_HEADER, self.client_id.as_str())
            .bearer(Secret::new(token.expose()));
        for (name, value) in &request.query {
            outbound = outbound.query(name.as_str(), value.as_str());
        }
        for refusal in &request.expected_refusals {
            outbound = outbound.expect_refusal(refusal.clone());
        }
        if let Some(body) = &request.body {
            let body = RequestBody::json(body)
                .map_err(|error| HelixError::Transport(error.to_string()))?;
            outbound = outbound.body(body);
        }
        Ok(outbound)
    }

    async fn attempt(
        &self,
        request: &HelixRequest,
        token: &OAuthToken,
    ) -> Result<serde_json::Value, HelixError> {
        let response = match self.gateway.send(self.outbound(request, token)?).await {
            Ok(response) => response,
            Err(failure @ GatewayFailure::Unauthorized { .. }) => {
                debug!(path = request.path, "helix rejected the token");
                return Err(failure.into());
            }
            Err(failure) => {
                warn!(path = request.path, error = %failure, "helix request failed");
                return Err(failure.into());
            }
        };
        debug!(method = ?request.method, path = request.path, status = response.status(), "helix response");
        if response.body().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        response
            .json()
            .map_err(|error| HelixError::UndecodableResponse(error.to_string()))
    }
}

#[async_trait]
impl HelixTransport for HelixHttpTransport {
    async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
        let token = self.tokens.access_token().await?;
        match self.attempt(&request, &token).await {
            Err(HelixError::ReauthRequired) => match &self.refresher {
                Some(refresher) => {
                    debug!(
                        path = request.path,
                        "helix rejected the token; refreshing once and retrying"
                    );
                    let fresh = refresher.refresh(&token).await?;
                    self.attempt(&request, &fresh).await
                }
                None => Err(HelixError::ReauthRequired),
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::helix_gateway::helix_gateway;
    use crate::sub_actions::test_support::endpoints_with;
    use forge_events::{Event, EventPublisher, EventSource};
    use forge_net::RateLimiter;
    use forge_net::test_support::RecordingRateLimiter;
    use forge_platform_core::EndpointSurface;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SENTINEL: &str = "FAKE_HELIX_TOKEN_SENTINEL_qq123";
    const FRESH: &str = "FAKE_HELIX_FRESH_TOKEN_zz789";
    const CLIENT_ID: &str = "test-client-id";
    const INTERACTIVE_BODY_CAP: usize = 4 * 1024 * 1024;

    struct StaticTokenSource;

    #[async_trait]
    impl HelixTokenSource for StaticTokenSource {
        async fn access_token(&self) -> Result<OAuthToken, HelixError> {
            Ok(OAuthToken::new(SENTINEL))
        }
    }

    struct FailingTokenSource;

    #[async_trait]
    impl HelixTokenSource for FailingTokenSource {
        async fn access_token(&self) -> Result<OAuthToken, HelixError> {
            Err(HelixError::Credentials("no twitch credentials".to_owned()))
        }
    }

    #[derive(Default)]
    struct CountingRefresher {
        calls: AtomicU32,
    }

    #[async_trait]
    impl HelixTokenRefresher for CountingRefresher {
        async fn refresh(&self, _failed_token: &OAuthToken) -> Result<OAuthToken, HelixError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(OAuthToken::new(FRESH))
        }
    }

    #[derive(Default)]
    struct RecordingBus(Mutex<Vec<Event>>);

    impl EventPublisher for RecordingBus {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl RecordingBus {
        fn request_fails(&self) -> Vec<Event> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.kind == "request.fail")
                .cloned()
                .collect()
        }

        fn single_request_fail(&self) -> Event {
            let mut events = self.request_fails();
            assert_eq!(events.len(), 1, "expected exactly one request.fail");
            events.remove(0)
        }
    }

    struct Harness {
        transport: HelixHttpTransport,
        limiter: Arc<RecordingRateLimiter>,
        bus: Arc<RecordingBus>,
    }

    fn harness_with(
        base_url: &str,
        limiter: Arc<RecordingRateLimiter>,
        tokens: Arc<dyn HelixTokenSource>,
    ) -> Harness {
        let bus = Arc::new(RecordingBus::default());
        let endpoints = endpoints_with(&[(EndpointSurface::TwitchApi, base_url)]);
        let gateway = helix_gateway(
            &endpoints,
            Arc::clone(&limiter) as Arc<dyn RateLimiter>,
            Arc::clone(&bus) as Arc<dyn EventPublisher>,
        )
        .unwrap();
        Harness {
            transport: HelixHttpTransport::new(gateway, CLIENT_ID.to_owned(), tokens),
            limiter,
            bus,
        }
    }

    fn harness(base_url: &str) -> Harness {
        harness_with(
            base_url,
            Arc::new(RecordingRateLimiter::granting()),
            Arc::new(StaticTokenSource),
        )
    }

    fn users() -> HelixRequest {
        HelixRequest::new(HelixMethod::Get, "/helix/users")
    }

    async fn server_answering(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn execute_sends_auth_headers_and_query_to_the_twitch_api_endpoint() {
        let server = MockServer::start().await;
        let payload = serde_json::json!({"data": [{"id": "42"}]});
        Mock::given(method("GET"))
            .and(path("/helix/users"))
            .and(query_param("login", "someone"))
            .and(header(
                "Authorization",
                format!("Bearer {SENTINEL}").as_str(),
            ))
            .and(header("Client-Id", CLIENT_ID))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload.clone()))
            .mount(&server)
            .await;
        let h = harness(&server.uri());

        let value = h
            .transport
            .execute(users().query("login", "someone"))
            .await
            .unwrap();

        assert_eq!(value, payload);
    }

    #[tokio::test]
    async fn execute_sends_the_json_body_on_writes() {
        let server = MockServer::start().await;
        let body = serde_json::json!({"data": {"user_id": "7", "reason": "spam"}});
        Mock::given(method("POST"))
            .and(path("/helix/moderation/bans"))
            .and(wiremock::matchers::body_json(body.clone()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": 1})))
            .mount(&server)
            .await;
        let h = harness(&server.uri());

        let value = h
            .transport
            .execute(HelixRequest::new(HelixMethod::Post, "/helix/moderation/bans").body(body))
            .await
            .unwrap();

        assert_eq!(value["ok"], 1);
    }

    #[tokio::test]
    async fn execute_returns_null_for_empty_success_body() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/helix/moderation/bans"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let h = harness(&server.uri());

        let value = h
            .transport
            .execute(HelixRequest::new(
                HelixMethod::Delete,
                "/helix/moderation/bans",
            ))
            .await
            .unwrap();

        assert_eq!(value, serde_json::Value::Null);
    }

    #[tokio::test]
    async fn malformed_success_body_maps_to_undecodable_response() {
        let server = server_answering(ResponseTemplate::new(200).set_body_string("not-json")).await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(matches!(err, HelixError::UndecodableResponse(_)));
    }

    #[tokio::test]
    async fn success_body_over_the_interactive_cap_maps_to_transport_error() {
        let oversized = vec![b' '; INTERACTIVE_BODY_CAP + 1];
        let server = server_answering(ResponseTemplate::new(200).set_body_bytes(oversized)).await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(matches!(err, HelixError::Transport(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn forbidden_response_maps_to_http_error_carrying_status_and_body() {
        let server = server_answering(
            ResponseTemplate::new(403).set_body_string(r#"{"error":"Forbidden"}"#),
        )
        .await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        match err {
            HelixError::Http { status, body } => {
                assert_eq!(status, 403);
                assert!(body.contains("Forbidden"), "body: {body}");
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_error_body_is_a_bounded_excerpt_with_the_bearer_masked() {
        let echoed = format!("{{\"echo\":\"{SENTINEL}\"}}{}", "x".repeat(2000));
        let server =
            server_answering(ResponseTemplate::new(500).set_body_string(echoed.clone())).await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        let HelixError::Http { status, body } = err else {
            panic!("expected Http error");
        };
        assert_eq!(status, 500);
        assert!(!body.contains(SENTINEL), "bearer leaked into body: {body}");
        assert!(body.starts_with("{\"echo\":\""), "body: {body}");
        assert!(body.chars().count() < echoed.chars().count());
    }

    #[tokio::test]
    async fn request_fail_payload_keeps_its_keys_and_carries_the_failure() {
        let server = server_answering(
            ResponseTemplate::new(403).set_body_string(r#"{"error":"Forbidden"}"#),
        )
        .await;
        let h = harness(&server.uri());

        h.transport.execute(users()).await.unwrap_err();

        let event = h.bus.single_request_fail();
        let keys: BTreeSet<&str> = event
            .payload
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(event.source, EventSource::Twitch);
        assert_eq!(
            keys,
            BTreeSet::from([
                "body_snippet",
                "endpoint",
                "retry_after_secs",
                "status_code"
            ])
        );
        assert_eq!(event.payload["endpoint"], "/helix/users");
        assert_eq!(event.payload["status_code"], 403);
        assert_eq!(event.payload["retry_after_secs"], serde_json::Value::Null);
        assert!(
            event.payload["body_snippet"]
                .as_str()
                .unwrap()
                .contains("Forbidden")
        );
    }

    #[tokio::test]
    async fn request_fail_body_snippet_is_cut_at_200_chars_not_bytes() {
        let server =
            server_answering(ResponseTemplate::new(500).set_body_string("я".repeat(300))).await;
        let h = harness(&server.uri());

        h.transport.execute(users()).await.unwrap_err();

        let event = h.bus.single_request_fail();
        let snippet = event.payload["body_snippet"].as_str().unwrap();
        assert_eq!(snippet.chars().count(), 200);
    }

    #[tokio::test]
    async fn request_fail_is_not_published_for_failures_without_a_status() {
        let oversized = vec![b' '; INTERACTIVE_BODY_CAP + 1];
        let server = server_answering(ResponseTemplate::new(200).set_body_bytes(oversized)).await;
        let cases = [
            harness(&server.uri()),
            harness("http://127.0.0.1:1"),
            harness_with(
                &server.uri(),
                Arc::new(RecordingRateLimiter::exhausted()),
                Arc::new(StaticTokenSource),
            ),
        ];

        for h in cases {
            h.transport.execute(users()).await.unwrap_err();
            assert!(h.bus.request_fails().is_empty());
        }
    }

    #[tokio::test]
    async fn too_many_requests_takes_its_wait_from_ratelimit_reset_then_retry_after_then_five_seconds()
     {
        let cases: [(&[(&str, &str)], u64); 3] = [
            (&[("Ratelimit-Reset", "1"), ("Retry-After", "7")], 0),
            (&[("Retry-After", "7")], 7),
            (&[], 5),
        ];
        for (headers, expected_secs) in cases {
            let mut response = ResponseTemplate::new(429);
            for (name, value) in headers {
                response = response.insert_header(*name, *value);
            }
            let server = server_answering(response).await;
            let h = harness(&server.uri());

            let err = h.transport.execute(users()).await.unwrap_err();

            assert!(
                matches!(err, HelixError::RateLimited),
                "{headers:?}: {err:?}"
            );
            assert_eq!(
                h.limiter.observed_throttles(),
                vec![Duration::from_secs(expected_secs)],
                "{headers:?}"
            );
            let event = h.bus.single_request_fail();
            assert_eq!(event.payload["status_code"], 429);
            assert_eq!(
                event.payload["retry_after_secs"], expected_secs,
                "{headers:?}"
            );
        }
    }

    #[tokio::test]
    async fn service_unavailable_with_only_ratelimit_reset_is_an_http_error_without_cooldown() {
        let server = server_answering(
            ResponseTemplate::new(503).insert_header("Ratelimit-Reset", "4102444800"),
        )
        .await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(
            matches!(err, HelixError::Http { status: 503, .. }),
            "got {err:?}"
        );
        assert!(h.limiter.observed_throttles().is_empty());
    }

    #[tokio::test]
    async fn service_unavailable_with_retry_after_cools_the_limiter_but_stays_an_http_error() {
        let server =
            server_answering(ResponseTemplate::new(503).insert_header("Retry-After", "7")).await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(
            matches!(err, HelixError::Http { status: 503, .. }),
            "got {err:?}"
        );
        assert_eq!(h.limiter.observed_throttles(), vec![Duration::from_secs(7)]);
        assert_eq!(h.bus.single_request_fail().payload["retry_after_secs"], 7);
    }

    #[tokio::test]
    async fn unauthorized_without_refresher_maps_to_reauth_required() {
        let server = server_answering(ResponseTemplate::new(401)).await;
        let h = harness(&server.uri());

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(matches!(err, HelixError::ReauthRequired));
    }

    #[tokio::test]
    async fn unauthorized_refreshes_once_and_retries_with_the_fresh_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header(
                "Authorization",
                format!("Bearer {SENTINEL}").as_str(),
            ))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(header("Authorization", format!("Bearer {FRESH}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": 1})))
            .mount(&server)
            .await;
        let refresher = Arc::new(CountingRefresher::default());
        let h = harness(&server.uri());
        let transport = h
            .transport
            .with_refresher(Arc::clone(&refresher) as Arc<dyn HelixTokenRefresher>);

        let value = transport.execute(users()).await.unwrap();

        assert_eq!(value["ok"], 1);
        assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unauthorized_after_the_refresh_gives_up_with_reauth_required() {
        let server = server_answering(ResponseTemplate::new(401)).await;
        let refresher = Arc::new(CountingRefresher::default());
        let h = harness(&server.uri());
        let transport = h
            .transport
            .with_refresher(Arc::clone(&refresher) as Arc<dyn HelixTokenRefresher>);

        let err = transport.execute(users()).await.unwrap_err();

        assert!(matches!(err, HelixError::ReauthRequired));
        assert_eq!(refresher.calls.load(Ordering::SeqCst), 1);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn exhausted_local_limiter_maps_to_rate_limited_without_network_call() {
        let server = MockServer::start().await;
        let h = harness_with(
            &server.uri(),
            Arc::new(RecordingRateLimiter::exhausted()),
            Arc::new(StaticTokenSource),
        );

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(matches!(err, HelixError::RateLimited));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn token_source_failure_propagates_without_network_call() {
        let server = MockServer::start().await;
        let h = harness_with(
            &server.uri(),
            Arc::new(RecordingRateLimiter::granting()),
            Arc::new(FailingTokenSource),
        );

        let err = h.transport.execute(users()).await.unwrap_err();

        assert!(matches!(err, HelixError::Credentials(_)));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn refused_connection_yields_transport_error_without_host_or_token() {
        let h = harness("http://127.0.0.1:1");

        let err = h.transport.execute(users()).await.unwrap_err();

        let display = err.to_string();
        assert!(matches!(err, HelixError::Transport(_)));
        assert!(!display.contains("127.0.0.1"), "{display}");
        assert!(!display.contains(SENTINEL), "{display}");
    }
}
