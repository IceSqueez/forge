use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use serde::{Deserialize, Serialize};

const REWARDS_ENDPOINT: &str = "https://api.kick.com/public/v1/channels/rewards";

pub struct KickRewards {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    rewards_endpoint: String,
}

pub struct CreateRewardParams {
    pub title: String,
    pub cost: u64,
    pub description: Option<String>,
    pub background_color: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_user_input_required: Option<bool>,
    pub should_redemptions_skip_request_queue: Option<bool>,
}

pub struct UpdateRewardParams {
    pub title: Option<String>,
    pub cost: Option<u64>,
    pub description: Option<String>,
    pub background_color: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_paused: Option<bool>,
    pub is_user_input_required: Option<bool>,
    pub should_redemptions_skip_request_queue: Option<bool>,
}

#[derive(Serialize)]
struct CreateBody {
    title: String,
    cost: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_user_input_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    should_redemptions_skip_request_queue: Option<bool>,
}

#[derive(Serialize)]
struct UpdateBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    background_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_paused: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_user_input_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    should_redemptions_skip_request_queue: Option<bool>,
}

#[derive(Deserialize)]
struct CreateResponse {
    id: Option<String>,
    data: Option<RewardData>,
}

#[derive(Deserialize)]
struct RewardData {
    id: Option<String>,
}

impl KickRewards {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            rewards_endpoint: REWARDS_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, base: String) -> Self {
        self.rewards_endpoint = format!("{base}/channels/rewards");
        self
    }

    pub async fn create(
        &self,
        params: CreateRewardParams,
        token: &str,
    ) -> Result<Option<String>, PlatformError> {
        self.acquire_slot().await?;

        let body = CreateBody {
            title: params.title,
            cost: params.cost,
            description: params.description,
            background_color: params.background_color,
            is_enabled: params.is_enabled,
            is_user_input_required: params.is_user_input_required,
            should_redemptions_skip_request_queue: params.should_redemptions_skip_request_queue,
        };

        let response = self
            .client
            .post(&self.rewards_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            let created_id = response
                .json::<CreateResponse>()
                .await
                .ok()
                .and_then(|r| r.id.or_else(|| r.data.and_then(|d| d.id)));
            return Ok(created_id);
        }

        map_rewards_error(status, response).await
    }

    pub async fn update(
        &self,
        reward_id: &str,
        params: UpdateRewardParams,
        token: &str,
    ) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let body = UpdateBody {
            title: params.title,
            cost: params.cost,
            description: params.description,
            background_color: params.background_color,
            is_enabled: params.is_enabled,
            is_paused: params.is_paused,
            is_user_input_required: params.is_user_input_required,
            should_redemptions_skip_request_queue: params.should_redemptions_skip_request_queue,
        };

        let url = format!("{}/{}", self.rewards_endpoint, reward_id);
        let response = self
            .client
            .patch(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }

        map_rewards_error(status, response).await
    }

    pub async fn delete(&self, reward_id: &str, token: &str) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let url = format!("{}/{}", self.rewards_endpoint, reward_id);
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
        if (200..300).contains(&status) {
            return Ok(());
        }

        map_rewards_error(status, response).await
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

async fn map_rewards_error<T>(
    status: u16,
    response: reqwest::Response,
) -> Result<T, PlatformError> {
    let retry_after_secs = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(30);

    let body = response.text().await.unwrap_or_default();

    match status {
        401 => Err(PlatformError::Auth {
            reason: "rewards token rejected (401)".to_owned(),
        }),
        403 => Err(PlatformError::Auth {
            reason:
                "rewards forbidden (403); check channel:rewards:write scope or reward ownership"
                    .to_owned(),
        }),
        400 | 422 => Err(PlatformError::Http { status, body }),
        429 => Err(PlatformError::RateLimited { retry_after_secs }),
        _ => Err(PlatformError::Http { status, body }),
    }
}
