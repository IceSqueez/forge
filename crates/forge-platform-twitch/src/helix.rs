use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use forge_runtime::EventBus;
use forge_types::OAuthToken;
use thiserror::Error;

const HELIX_BASE_URL: &str = "https://api.twitch.tv";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_SNIPPET_MAX_CHARS: usize = 200;
/// Upper bound on a single throttle sleep and on the cumulative wait across
/// retries: we would rather surface `RateLimited` to the caller than block a
/// request for minutes when the budget is deeply exhausted.
const MAX_THROTTLE_WAIT: Duration = Duration::from_secs(10);
/// Bound on acquire attempts so a misbehaving limiter cannot spin forever.
const MAX_ACQUIRE_ATTEMPTS: u32 = 3;
/// Used when a 429 omits `Retry-After`; a conservative default back-off.
const DEFAULT_RETRY_AFTER_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelixMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

#[derive(Debug, Clone)]
pub struct HelixRequest {
    pub method: HelixMethod,
    /// Host-relative, starts with `/helix/`.
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
}

impl HelixRequest {
    pub fn new(method: HelixMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: Vec::new(),
            body: None,
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
}

/// Display never contains a bearer token or a request URL.
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
}

#[async_trait]
pub trait HelixTokenSource: Send + Sync {
    /// Called once per request, so a rotated token takes effect without rebuilding the transport.
    async fn access_token(&self) -> Result<OAuthToken, HelixError>;
}

/// Renews the stored token after a Helix 401; a rejected refresh token
/// surfaces as `ReauthRequired`.
#[async_trait]
pub trait HelixTokenRefresher: Send + Sync {
    async fn refresh(&self) -> Result<OAuthToken, HelixError>;
}

/// Implementations own rate-limit acquisition, auth headers, and failure
/// telemetry: every non-2xx response publishes a `request.fail` bus event
/// (endpoint path, status code, body snippet, retry-after) before the error
/// is returned. An empty success body yields `serde_json::Value::Null`.
#[async_trait]
pub trait HelixTransport: Send + Sync {
    async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError>;
}

pub struct HelixHttpTransport {
    http: reqwest::Client,
    rate_limiter: Arc<dyn RateLimiter>,
    bus: Arc<EventBus>,
    client_id: String,
    tokens: Arc<dyn HelixTokenSource>,
    refresher: Option<Arc<dyn HelixTokenRefresher>>,
    base_url: String,
}

impl HelixHttpTransport {
    pub fn new(
        rate_limiter: Arc<dyn RateLimiter>,
        bus: Arc<EventBus>,
        client_id: String,
        tokens: Arc<dyn HelixTokenSource>,
    ) -> Self {
        Self::with_base_url(
            HELIX_BASE_URL.to_owned(),
            rate_limiter,
            bus,
            client_id,
            tokens,
        )
    }

    /// Enables the reactive 401 path: a single refresh-then-retry per request
    /// before falling through to `ReauthRequired`.
    pub fn with_refresher(mut self, refresher: Arc<dyn HelixTokenRefresher>) -> Self {
        self.refresher = Some(refresher);
        self
    }

    pub(crate) fn with_base_url(
        base_url: String,
        rate_limiter: Arc<dyn RateLimiter>,
        bus: Arc<EventBus>,
        client_id: String,
        tokens: Arc<dyn HelixTokenSource>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            rate_limiter,
            bus,
            client_id,
            tokens,
            refresher: None,
            base_url,
        }
    }

    fn publish_request_fail(&self, path: &str, status: u16, body: &str, retry_after: Option<u64>) {
        let body_snippet: String = body.chars().take(BODY_SNIPPET_MAX_CHARS).collect();
        self.bus.publish(Event::new(
            EventSource::Twitch,
            "request.fail",
            serde_json::json!({
                "endpoint": path,
                "status_code": status,
                "body_snippet": body_snippet,
                "retry_after_secs": retry_after,
            }),
        ));
    }
}

impl HelixHttpTransport {
    /// Acquires one rate-limit point and issues the request with `token`.
    /// Returns `Err(ReauthRequired)` on a 401 so the caller can decide whether
    /// a refresh-then-retry is still available.
    async fn attempt(
        &self,
        request: &HelixRequest,
        token: &OAuthToken,
    ) -> Result<serde_json::Value, HelixError> {
        // Acquire one point, sleeping over short throttles. The cumulative wait
        // is bounded by MAX_THROTTLE_WAIT so the caller never blocks for long;
        // beyond that we report RateLimited and let the caller decide.
        let mut waited = Duration::ZERO;
        let mut attempts = 0;
        loop {
            let outcome = self
                .rate_limiter
                .acquire(1)
                .await
                .map_err(|_| HelixError::RateLimited)?;
            match outcome {
                RateLimitOutcome::Granted => break,
                RateLimitOutcome::Throttled { wait_for } => {
                    attempts += 1;
                    if attempts >= MAX_ACQUIRE_ATTEMPTS || waited >= MAX_THROTTLE_WAIT {
                        return Err(HelixError::RateLimited);
                    }
                    let sleep_for = wait_for.min(MAX_THROTTLE_WAIT);
                    tokio::time::sleep(sleep_for).await;
                    waited += sleep_for;
                }
                RateLimitOutcome::Exhausted => return Err(HelixError::RateLimited),
            }
        }

        let method = match request.method {
            HelixMethod::Get => reqwest::Method::GET,
            HelixMethod::Post => reqwest::Method::POST,
            HelixMethod::Patch => reqwest::Method::PATCH,
            HelixMethod::Put => reqwest::Method::PUT,
            HelixMethod::Delete => reqwest::Method::DELETE,
        };

        let url = format!("{}{}", self.base_url, request.path);
        let mut builder = self
            .http
            .request(method, url)
            .header("Authorization", format!("Bearer {}", token.expose()))
            .header("Client-Id", &self.client_id);
        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        if let Some(body) = &request.body {
            builder = builder.json(body);
        }

        let resp = tokio::time::timeout(REQUEST_TIMEOUT, builder.send())
            .await
            .map_err(|_| HelixError::Transport("request timed out".to_owned()))?
            .map_err(|e| HelixError::Transport(e.without_url().to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let retry_after = extract_retry_after(&resp);
            let body_text = resp.text().await.unwrap_or_default();
            self.publish_request_fail(&request.path, status, &body_text, retry_after);
            if status == 401 {
                return Err(HelixError::ReauthRequired);
            }
            if status == 429 {
                // Feed the server's back-off into the shared bucket so every
                // transport sharing this limiter stops hammering Helix.
                let cooldown = retry_after.unwrap_or(DEFAULT_RETRY_AFTER_SECS);
                self.rate_limiter
                    .observe_remote_throttle(Duration::from_secs(cooldown))
                    .await;
                return Err(HelixError::RateLimited);
            }
            return Err(HelixError::Http {
                status,
                body: body_text,
            });
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| HelixError::Transport(e.without_url().to_string()))?;
        if bytes.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| HelixError::Transport(e.to_string()))
    }
}

#[async_trait]
impl HelixTransport for HelixHttpTransport {
    async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
        let token = self.tokens.access_token().await?;
        match self.attempt(&request, &token).await {
            Err(HelixError::ReauthRequired) => match &self.refresher {
                // Single bounded retry: refresh once, then re-issue. A second
                // 401 after a successful refresh is terminal, so a token Twitch
                // keeps rejecting cannot loop.
                Some(refresher) => {
                    let fresh = refresher.refresh().await?;
                    self.attempt(&request, &fresh).await
                }
                None => Err(HelixError::ReauthRequired),
            },
            other => other,
        }
    }
}

/// Enforces no Helix request budget; every acquire is granted immediately.
pub struct NoopRateLimiter;

#[async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        Ok(RateLimitOutcome::Granted)
    }

    fn remaining(&self) -> u32 {
        u32::MAX
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

fn extract_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_platform_core::PlatformError;
    use forge_runtime::NullEventLogRepo;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SENTINEL: &str = "FAKE_HELIX_TOKEN_SENTINEL_qq123";
    const CLIENT_ID: &str = "test-client-id";

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

    struct GrantLimiter;

    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }

        fn remaining(&self) -> u32 {
            u32::MAX
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    struct ExhaustedLimiter;

    #[async_trait]
    impl RateLimiter for ExhaustedLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Exhausted)
        }

        fn remaining(&self) -> u32 {
            0
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    /// Throttles the first `throttle_count` acquires (each with `wait_for`),
    /// then grants. Lets us drive the transport's throttle-sleep loop on the
    /// paused tokio clock without any wall-clock dependence.
    struct ThrottleThenGrantLimiter {
        wait_for: Duration,
        throttle_count: u32,
        seen: std::sync::atomic::AtomicU32,
    }

    #[async_trait]
    impl RateLimiter for ThrottleThenGrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            let n = self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < self.throttle_count {
                Ok(RateLimitOutcome::Throttled {
                    wait_for: self.wait_for,
                })
            } else {
                Ok(RateLimitOutcome::Granted)
            }
        }

        fn remaining(&self) -> u32 {
            u32::MAX
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    /// Always throttles, so the transport must give up after its bounded
    /// acquire attempts rather than spin forever.
    struct AlwaysThrottleLimiter {
        wait_for: Duration,
    }

    #[async_trait]
    impl RateLimiter for AlwaysThrottleLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Throttled {
                wait_for: self.wait_for,
            })
        }

        fn remaining(&self) -> u32 {
            0
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    /// Records every `observe_remote_throttle` invocation and its argument, so
    /// a test can prove the 429 path fed the server's back-off into the bucket.
    struct RecordingLimiter {
        observed: std::sync::Mutex<Vec<Duration>>,
    }

    impl RecordingLimiter {
        fn new() -> Self {
            Self {
                observed: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl RateLimiter for RecordingLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }

        fn remaining(&self) -> u32 {
            u32::MAX
        }

        async fn observe_remote_throttle(&self, retry_after: Duration) {
            self.observed.lock().unwrap().push(retry_after);
        }
    }

    fn transport(base_url: String) -> (HelixHttpTransport, Arc<EventBus>) {
        transport_with(
            base_url,
            Arc::new(GrantLimiter),
            Arc::new(StaticTokenSource),
        )
    }

    fn transport_with(
        base_url: String,
        limiter: Arc<dyn RateLimiter>,
        tokens: Arc<dyn HelixTokenSource>,
    ) -> (HelixHttpTransport, Arc<EventBus>) {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let t = HelixHttpTransport::with_base_url(
            base_url,
            limiter,
            Arc::clone(&bus),
            CLIENT_ID.to_owned(),
            tokens,
        );
        (t, bus)
    }

    async fn recv_request_fail(sub: &mut forge_runtime::EventSubscription) -> Event {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let event = sub.recv().await.unwrap();
                if event.kind == "request.fail" {
                    return event;
                }
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn execute_sends_auth_headers_and_returns_json_on_2xx() {
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
        let (t, _bus) = transport(server.uri());

        let value = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users").query("login", "someone"))
            .await
            .unwrap();

        assert_eq!(value, payload);
    }

    #[tokio::test]
    async fn execute_returns_null_for_empty_success_body() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/helix/moderation/bans"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let (t, _bus) = transport(server.uri());

        let value = t
            .execute(HelixRequest::new(
                HelixMethod::Delete,
                "/helix/moderation/bans",
            ))
            .await
            .unwrap();

        assert_eq!(value, serde_json::Value::Null);
    }

    #[tokio::test]
    async fn forbidden_response_yields_http_error_and_request_fail_event() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/helix/moderation/banned"))
            .respond_with(ResponseTemplate::new(403).set_body_string(r#"{"error":"Forbidden"}"#))
            .mount(&server)
            .await;
        let (t, bus) = transport(server.uri());
        let mut sub = bus.subscribe();

        let err = t
            .execute(HelixRequest::new(
                HelixMethod::Get,
                "/helix/moderation/banned",
            ))
            .await
            .unwrap_err();

        let display = err.to_string();
        assert!(
            !display.contains(SENTINEL),
            "error display must not leak the token: {display}"
        );
        match &err {
            HelixError::Http { status, body } => {
                assert_eq!(*status, 403);
                assert!(body.contains("Forbidden"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }

        let event = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "request.fail");
        assert_eq!(event.source, EventSource::Twitch);
        assert_eq!(event.payload["endpoint"], "/helix/moderation/banned");
        assert_eq!(event.payload["status_code"], 403);
        assert!(
            event.payload["body_snippet"]
                .as_str()
                .unwrap()
                .contains("Forbidden")
        );
    }

    #[tokio::test]
    async fn unauthorized_response_maps_to_reauth_required() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let (t, _bus) = transport(server.uri());

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::ReauthRequired));
    }

    #[tokio::test]
    async fn too_many_requests_maps_to_rate_limited_with_retry_after_in_event() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "7"))
            .mount(&server)
            .await;
        let (t, bus) = transport(server.uri());
        let mut sub = bus.subscribe();

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::RateLimited));
        let event = recv_request_fail(&mut sub).await;
        assert_eq!(event.payload["retry_after_secs"], 7);
        assert_eq!(event.payload["status_code"], 429);
    }

    #[tokio::test]
    async fn exhausted_local_limiter_short_circuits_without_network_call() {
        let server = MockServer::start().await;
        let (t, _bus) = transport_with(
            server.uri(),
            Arc::new(ExhaustedLimiter),
            Arc::new(StaticTokenSource),
        );

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::RateLimited));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no HTTP request may be issued when the local limiter is exhausted"
        );
    }

    #[tokio::test]
    async fn token_source_failure_propagates_without_network_call() {
        let server = MockServer::start().await;
        let (t, _bus) = transport_with(
            server.uri(),
            Arc::new(GrantLimiter),
            Arc::new(FailingTokenSource),
        );

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::Credentials(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no HTTP request may be issued without an access token"
        );
    }

    #[tokio::test]
    async fn request_fail_event_truncates_body_snippet_to_200_chars() {
        let long_body = "я".repeat(300); // multibyte: truncation must count chars, not bytes
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500).set_body_string(long_body.clone()))
            .mount(&server)
            .await;
        let (t, bus) = transport(server.uri());
        let mut sub = bus.subscribe();

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        match err {
            HelixError::Http { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body.chars().count(), 300, "error keeps the full body");
            }
            other => panic!("expected Http error, got {other:?}"),
        }
        let event = recv_request_fail(&mut sub).await;
        assert_eq!(
            event.payload["body_snippet"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            200
        );
    }

    #[tokio::test]
    async fn malformed_success_body_maps_to_transport_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        let (t, _bus) = transport(server.uri());

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::Transport(_)));
    }

    #[tokio::test]
    async fn refused_connection_yields_transport_error_without_host_or_token() {
        // Port 1 needs root to bind, so the connection is refused immediately.
        let (t, _bus) = transport("http://127.0.0.1:1".to_owned());

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        let display = err.to_string();
        assert!(matches!(err, HelixError::Transport(_)));
        assert!(
            !display.contains("127.0.0.1"),
            "transport error must strip the request URL: {display}"
        );
        assert!(
            !display.contains(SENTINEL),
            "transport error must not leak the token: {display}"
        );
    }

    #[tokio::test]
    async fn transient_throttle_is_slept_off_then_request_succeeds() {
        let server = MockServer::start().await;
        let payload = serde_json::json!({"data": []});
        Mock::given(method("GET"))
            .and(path("/helix/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload.clone()))
            .mount(&server)
            .await;
        // One throttle then grant. A zero `wait_for` makes the loop's
        // `sleep(ZERO)` return on the next poll with no real elapsed time, so
        // the test exercises the retry path without any wall-clock dependence.
        let limiter = Arc::new(ThrottleThenGrantLimiter {
            wait_for: Duration::ZERO,
            throttle_count: 1,
            seen: std::sync::atomic::AtomicU32::new(0),
        });
        let (t, _bus) = transport_with(server.uri(), limiter.clone(), Arc::new(StaticTokenSource));

        let value = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap();

        assert_eq!(value, payload);
        assert_eq!(
            limiter.seen.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "limiter must be polled twice: one throttle, one grant"
        );
    }

    #[tokio::test]
    async fn persistent_throttle_gives_up_with_rate_limited() {
        let server = MockServer::start().await;
        // No HTTP request should ever be issued; the loop exits after the
        // bounded attempt count. A zero `wait_for` keeps each retry's
        // `sleep(ZERO)` instantaneous, so termination is by attempt count, not
        // by elapsed time.
        let limiter = Arc::new(AlwaysThrottleLimiter {
            wait_for: Duration::ZERO,
        });
        let (t, _bus) = transport_with(server.uri(), limiter, Arc::new(StaticTokenSource));

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::RateLimited));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a request stuck in the throttle loop must not reach the network"
        );
    }

    #[tokio::test]
    async fn too_many_requests_feeds_retry_after_into_limiter() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "11"))
            .mount(&server)
            .await;
        let limiter = Arc::new(RecordingLimiter::new());
        let (t, _bus) = transport_with(server.uri(), limiter.clone(), Arc::new(StaticTokenSource));

        let err = t
            .execute(HelixRequest::new(HelixMethod::Get, "/helix/users"))
            .await
            .unwrap_err();

        assert!(matches!(err, HelixError::RateLimited));
        let observed = limiter.observed.lock().unwrap();
        assert_eq!(
            observed.as_slice(),
            &[Duration::from_secs(11)],
            "the parsed Retry-After must be pushed into the shared bucket exactly once"
        );
    }
}
