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
    #[allow(dead_code)]
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
