use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};

const BANS_ENDPOINT: &str = "https://api.kick.com/public/v1/moderation/bans";

pub struct KickModeration {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    bans_endpoint: String,
}

impl KickModeration {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            bans_endpoint: BANS_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api_base(mut self, base: String) -> Self {
        self.bans_endpoint = format!("{base}/moderation/bans");
        self
    }

    pub async fn ban(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        token: &str,
    ) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let response = self
            .client
            .post(&self.bans_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&serde_json::json!({
                "broadcaster_user_id": broadcaster_user_id,
                "user_id": target_user_id,
            }))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        map_moderation_response(response).await
    }

    pub async fn timeout(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        duration_minutes: u32,
        token: &str,
    ) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let response = self
            .client
            .post(&self.bans_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&serde_json::json!({
                "broadcaster_user_id": broadcaster_user_id,
                "user_id": target_user_id,
                "duration": duration_minutes,
            }))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        map_moderation_response(response).await
    }

    pub async fn unban(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        token: &str,
    ) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let response = self
            .client
            .delete(&self.bans_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&serde_json::json!({
                "broadcaster_user_id": broadcaster_user_id,
                "user_id": target_user_id,
            }))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        map_moderation_response(response).await
    }

    async fn acquire_slot(&self) -> Result<(), PlatformError> {
        let outcome = self
            .limiter
            .acquire(1)
            .await
            .map_err(|_| PlatformError::RateLimitExhausted)?;

        if matches!(outcome, RateLimitOutcome::Exhausted) {
            return Err(PlatformError::RateLimitExhausted);
        }

        Ok(())
    }
}

async fn map_moderation_response(response: reqwest::Response) -> Result<(), PlatformError> {
    let status = response.status().as_u16();
    if status == 200 || status == 201 || status == 204 {
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
            reason: "moderation token rejected (401)".to_owned(),
        }),
        403 => Err(PlatformError::Auth {
            reason: "moderation forbidden (403); check moderation:ban scope".to_owned(),
        }),
        429 => Err(PlatformError::RateLimited { retry_after_secs }),
        _ => Err(PlatformError::Http { status, body }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_platform_core::RateLimitOutcome;
    use std::time::Duration;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct GrantLimiter;
    #[async_trait::async_trait]
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
    #[async_trait::async_trait]
    impl RateLimiter for ExhaustedLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Exhausted)
        }
        fn remaining(&self) -> u32 {
            0
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn moderation_on(server: &MockServer) -> KickModeration {
        KickModeration::new(Arc::new(GrantLimiter)).with_api_base(server.uri())
    }

    async fn last_body(server: &MockServer) -> serde_json::Value {
        let reqs = server.received_requests().await.unwrap();
        let body = reqs.last().unwrap().body.clone();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn ban_posts_to_bans_endpoint_carrying_ids_without_duration() {
        for status in [200_u16, 201, 204] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/moderation/bans"))
                .respond_with(ResponseTemplate::new(status))
                .expect(1)
                .mount(&server)
                .await;

            let result = moderation_on(&server).ban(99, 42, "tok").await;
            assert!(result.is_ok(), "status {status} must map to Ok");

            let body = last_body(&server).await;
            assert_eq!(body["broadcaster_user_id"], 42);
            assert_eq!(body["user_id"], 99);
            assert!(
                body.get("duration").is_none(),
                "permanent ban must not carry a duration key"
            );
        }
    }

    #[tokio::test]
    async fn timeout_posts_body_carrying_duration_minutes_alongside_ids() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/moderation/bans"))
            .and(body_string_contains("duration"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let result = moderation_on(&server).timeout(99, 42, 15, "tok").await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["broadcaster_user_id"], 42);
        assert_eq!(body["user_id"], 99);
        assert_eq!(body["duration"], 15);
    }

    #[tokio::test]
    async fn unban_deletes_bans_endpoint_and_maps_2xx_to_ok() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let result = moderation_on(&server).unban(99, 42, "tok").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn auth_status_maps_to_auth_error() {
        for status in [401_u16, 403] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;

            let err = moderation_on(&server).ban(99, 42, "tok").await.unwrap_err();
            assert!(
                matches!(err, PlatformError::Auth { .. }),
                "status {status} must map to Auth, got {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn rate_limited_status_maps_to_rate_limited_with_parsed_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "57"))
            .mount(&server)
            .await;

        let err = moderation_on(&server).ban(99, 42, "tok").await.unwrap_err();
        assert!(matches!(
            err,
            PlatformError::RateLimited {
                retry_after_secs: 57
            }
        ));
    }

    #[tokio::test]
    async fn limiter_exhaustion_returns_rate_limit_exhausted_without_reaching_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = KickModeration::new(Arc::new(ExhaustedLimiter)).with_api_base(server.uri());
        let err = client.ban(99, 42, "tok").await.unwrap_err();

        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an exhausted limiter must short-circuit before any HTTP call"
        );
    }
}
