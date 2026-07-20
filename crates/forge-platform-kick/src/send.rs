use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimiter, acquire_or_wait};

const SEND_ENDPOINT: &str = "https://api.kick.com/public/v1/chat";
const DELETE_BASE: &str = "https://api.kick.com/public/v1/chat";

pub struct KickSendChat {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    send_endpoint: String,
    delete_base: String,
}

impl KickSendChat {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            send_endpoint: SEND_ENDPOINT.to_owned(),
            delete_base: DELETE_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_send_endpoint(mut self, url: String) -> Self {
        self.send_endpoint = url;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_delete_base(mut self, base: String) -> Self {
        self.delete_base = base;
        self
    }

    pub async fn send(
        &self,
        content: &str,
        token: &str,
        broadcaster_user_id: u64,
    ) -> Result<(), PlatformError> {
        acquire_or_wait(self.limiter.as_ref(), 1).await?;

        let response = self
            .client
            .post(&self.send_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&serde_json::json!({
                "content": content,
                "type": "user",
                "broadcaster_user_id": broadcaster_user_id,
            }))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
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

    pub async fn delete(&self, message_id: &str, token: &str) -> Result<(), PlatformError> {
        acquire_or_wait(self.limiter.as_ref(), 1).await?;

        let url = format!("{}/{}", self.delete_base, message_id);
        let response = self
            .client
            .delete(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if status == 204 {
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
                reason: "delete-chat-message token rejected (401)".to_owned(),
            }),
            403 => Err(PlatformError::Auth {
                reason: "delete-chat-message forbidden (403); check moderation:chat_message:manage scope".to_owned(),
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
            120
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

    fn grant_sender(server: &MockServer) -> KickSendChat {
        KickSendChat::new(Arc::new(GrantLimiter))
            .with_send_endpoint(format!("{}/chat", server.uri()))
    }

    #[tokio::test]
    async fn send_returns_auth_error_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "bad", 42)
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Auth { .. }));
    }

    #[tokio::test]
    async fn send_returns_rate_limited_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "30")
                    .set_body_string("too many requests"),
            )
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "tok", 42)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PlatformError::RateLimited {
                retry_after_secs: 30
            }
        ));
    }

    #[tokio::test]
    async fn send_uses_default_retry_after_when_header_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(429).set_body_string("too many"))
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "tok", 42)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PlatformError::RateLimited {
                retry_after_secs: 30
            }
        ));
    }

    #[tokio::test]
    async fn send_returns_rate_limit_exhausted_when_limiter_exhausted() {
        let server = MockServer::start().await;
        let sender = KickSendChat::new(Arc::new(ExhaustedLimiter))
            .with_send_endpoint(format!("{}/chat", server.uri()));

        let err = sender.send("hi", "tok", 42).await.unwrap_err();
        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_maps_500_to_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
            .mount(&server)
            .await;

        let err = grant_sender(&server)
            .send("hi", "tok", 42)
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Http { status: 500, .. }));
    }

    fn grant_deleter(server: &MockServer) -> KickSendChat {
        KickSendChat::new(Arc::new(GrantLimiter)).with_delete_base(format!("{}/chat", server.uri()))
    }

    #[tokio::test]
    async fn delete_targets_message_id_path_and_succeeds_on_204() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/chat/msg-77"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        grant_deleter(&server)
            .delete("msg-77", "tok")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_returns_auth_error_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/chat/msg-1"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let err = grant_deleter(&server)
            .delete("msg-1", "bad")
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Auth { .. }));
    }

    #[tokio::test]
    async fn delete_returns_auth_error_on_403() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/chat/msg-1"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let err = grant_deleter(&server)
            .delete("msg-1", "tok")
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Auth { .. }));
    }

    #[tokio::test]
    async fn delete_returns_rate_limited_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/chat/msg-1"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "12")
                    .set_body_string("slow down"),
            )
            .mount(&server)
            .await;

        let err = grant_deleter(&server)
            .delete("msg-1", "tok")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            PlatformError::RateLimited {
                retry_after_secs: 12
            }
        ));
    }

    #[tokio::test]
    async fn delete_returns_rate_limit_exhausted_without_reaching_server() {
        let server = MockServer::start().await;
        let sender = KickSendChat::new(Arc::new(ExhaustedLimiter))
            .with_delete_base(format!("{}/chat", server.uri()));

        let err = sender.delete("msg-1", "tok").await.unwrap_err();
        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
