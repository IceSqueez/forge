use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_platform_core::{RateLimitOutcome, RateLimiter};
use forge_runtime::EventBus;
use forge_types::OAuthToken;
use thiserror::Error;

const HELIX_BASE_URL: &str = "https://api.twitch.tv";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_SNIPPET_MAX_CHARS: usize = 200;

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

#[async_trait]
impl HelixTransport for HelixHttpTransport {
    async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
        let outcome = self
            .rate_limiter
            .acquire(1)
            .await
            .map_err(|_| HelixError::RateLimited)?;
        if matches!(outcome, RateLimitOutcome::Exhausted) {
            return Err(HelixError::RateLimited);
        }

        let token = self.tokens.access_token().await?;

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

fn extract_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}
