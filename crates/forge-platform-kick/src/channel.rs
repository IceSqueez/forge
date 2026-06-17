use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use serde::Serialize;

const CHANNELS_ENDPOINT: &str = "https://api.kick.com/public/v1/channels";

pub struct KickChannel {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    channels_endpoint: String,
}

#[derive(Serialize)]
struct UpdateBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_tags: Option<Vec<String>>,
}

impl KickChannel {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            channels_endpoint: CHANNELS_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, base: String) -> Self {
        self.channels_endpoint = format!("{base}/channels");
        self
    }

    pub async fn update_info(
        &self,
        token: &str,
        title: Option<String>,
        category_id: Option<u64>,
        tags: Option<Vec<String>>,
    ) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let body = UpdateBody {
            stream_title: title,
            category_id,
            custom_tags: tags,
        };

        let response = self
            .client
            .patch(&self.channels_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        map_channel_response(response).await
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

async fn map_channel_response(response: reqwest::Response) -> Result<(), PlatformError> {
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
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
            reason: "channel update token rejected (401)".to_owned(),
        }),
        403 => Err(PlatformError::Auth {
            reason: "channel update forbidden (403); check channel:write scope".to_owned(),
        }),
        400 | 422 => Err(PlatformError::Http { status, body }),
        429 => Err(PlatformError::RateLimited { retry_after_secs }),
        _ => Err(PlatformError::Http { status, body }),
    }
}
