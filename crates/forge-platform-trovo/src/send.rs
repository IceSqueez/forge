use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};

const SEND_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/chat/send";

pub struct TrovoSendChat {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    send_endpoint: String,
}

impl TrovoSendChat {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            send_endpoint: SEND_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_send_endpoint(mut self, url: String) -> Self {
        self.send_endpoint = url;
        self
    }

    pub async fn send(
        &self,
        content: &str,
        token: &str,
        client_id: &str,
    ) -> Result<(), PlatformError> {
        let outcome = self
            .limiter
            .acquire(1)
            .await
            .map_err(|_| PlatformError::RateLimitExhausted)?;

        if matches!(outcome, RateLimitOutcome::Exhausted) {
            return Err(PlatformError::RateLimitExhausted);
        }

        let response = self
            .client
            .post(&self.send_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .header("client-id", client_id)
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = response.status().as_u16();
        if status == 200 || status == 201 {
            return Ok(());
        }

        let retry_after_secs = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(30);

        let body = response.text().await.unwrap_or_default();

        match status {
            401 => Err(PlatformError::Auth {
                reason: "send-chat token rejected (401)".to_owned(),
            }),
            429 => Err(PlatformError::RateLimited { retry_after_secs }),
            _ => Err(PlatformError::Http { status, body }),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};

    use std::sync::Arc;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct GrantLimiter;

    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }

        fn remaining(&self) -> u32 {
            1200
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

    fn grant_sender(server: &MockServer) -> TrovoSendChat {
        TrovoSendChat::new(Arc::new(GrantLimiter))
            .with_send_endpoint(format!("{}/chat/send", server.uri()))
    }

    #[tokio::test]
    async fn send_returns_auth_error_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/send"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "bad_tok", "cid")
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { .. }),
            "expected Auth error on 401, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_returns_rate_limited_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/send"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "30")
                    .set_body_string("too many requests"),
            )
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "tok", "cid")
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                PlatformError::RateLimited {
                    retry_after_secs: 30
                }
            ),
            "expected RateLimited with retry_after_secs=30, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_uses_default_retry_after_when_header_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/send"))
            .respond_with(ResponseTemplate::new(429).set_body_string("too many requests"))
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "tok", "cid")
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                PlatformError::RateLimited {
                    retry_after_secs: 30
                }
            ),
            "expected default retry_after_secs=30, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_returns_rate_limit_exhausted_when_limiter_exhausted() {
        let server = MockServer::start().await;
        let sender = TrovoSendChat::new(Arc::new(ExhaustedLimiter))
            .with_send_endpoint(format!("{}/chat/send", server.uri()));

        let err = sender.send("hi", "tok", "cid").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::RateLimitExhausted),
            "expected RateLimitExhausted from exhausted limiter, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no HTTP request should be made when limiter is exhausted",
        );
    }

    #[tokio::test]
    async fn send_maps_500_to_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/send"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "tok", "cid")
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 500, .. }),
            "expected Http 500, got: {err}"
        );
    }
}
