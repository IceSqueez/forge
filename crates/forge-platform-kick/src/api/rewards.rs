use forge_net::{Gateway, GatewayRequest, GatewayResponse};
use forge_platform_core::{NON_HTTP_STATUS, PlatformError};
use serde::{Deserialize, Serialize};

use crate::gateway::{RefusalReasons, authorized, json_body, malformed, platform_error};

const REWARDS_PATH: &str = "/channels/rewards";
const REWARD_LABEL: &str = "/channels/rewards/{id}";
const REDEMPTIONS_PATH: &str = "/channels/rewards/redemptions";
const ACCEPT_REDEMPTIONS_PATH: &str = "/channels/rewards/redemptions/accept";
const REJECT_REDEMPTIONS_PATH: &str = "/channels/rewards/redemptions/reject";
const PENDING_STATUS: &str = "pending";
const MAX_REDEMPTION_BATCH: usize = 25;

const REWARDS_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "rewards token rejected (401)",
    forbidden: Some(
        "rewards forbidden (403); check channel:rewards:write scope or reward ownership",
    ),
};

pub struct KickRewards {
    api: Gateway,
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

#[derive(Serialize)]
struct RedemptionBatchBody<'a> {
    ids: &'a [String],
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

pub struct RedemptionRecord {
    pub id: String,
    pub reward_id: String,
    pub reward_title: String,
    pub redeemer_user_id: u64,
    pub redeemer_username: String,
    pub user_input: String,
}

#[derive(Deserialize)]
struct RedemptionsEnvelope {
    #[serde(default)]
    data: Vec<RedemptionData>,
}

#[derive(Deserialize, Default)]
struct RedemptionData {
    #[serde(default)]
    id: String,
    #[serde(default)]
    reward: RewardRef,
    #[serde(default)]
    redeemer: RedeemerRef,
    #[serde(default)]
    user_input: String,
}

#[derive(Deserialize, Default)]
struct RewardRef {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize, Default)]
struct RedeemerRef {
    #[serde(default)]
    user_id: u64,
    #[serde(default)]
    username: String,
}

impl KickRewards {
    pub fn new(api: Gateway) -> Self {
        Self { api }
    }

    pub async fn create(
        &self,
        params: CreateRewardParams,
        token: &str,
    ) -> Result<Option<String>, PlatformError> {
        let body = CreateBody {
            title: params.title,
            cost: params.cost,
            description: params.description,
            background_color: params.background_color,
            is_enabled: params.is_enabled,
            is_user_input_required: params.is_user_input_required,
            should_redemptions_skip_request_queue: params.should_redemptions_skip_request_queue,
        };
        let request = authorized(GatewayRequest::post(REWARDS_PATH), token, REWARDS_PATH)
            .body(json_body(&body)?);
        let response = self.send(request).await?;
        Ok(response
            .json::<CreateResponse>()
            .ok()
            .and_then(|r| r.id.or_else(|| r.data.and_then(|d| d.id))))
    }

    pub async fn update(
        &self,
        reward_id: &str,
        params: UpdateRewardParams,
        token: &str,
    ) -> Result<(), PlatformError> {
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
        let request = authorized(
            GatewayRequest::patch(reward_path(reward_id)),
            token,
            REWARD_LABEL,
        )
        .body(json_body(&body)?);
        self.send(request).await.map(drop)
    }

    pub async fn delete(&self, reward_id: &str, token: &str) -> Result<(), PlatformError> {
        let request = authorized(
            GatewayRequest::delete(reward_path(reward_id)),
            token,
            REWARD_LABEL,
        );
        self.send(request).await.map(drop)
    }

    pub async fn accept_redemptions(
        &self,
        ids: &[String],
        token: &str,
    ) -> Result<(), PlatformError> {
        check_redemption_batch("accept_redemptions", ids)?;
        self.post_redemption_batch(ACCEPT_REDEMPTIONS_PATH, ids, token)
            .await
    }

    pub async fn reject_redemptions(
        &self,
        ids: &[String],
        token: &str,
    ) -> Result<(), PlatformError> {
        check_redemption_batch("reject_redemptions", ids)?;
        self.post_redemption_batch(REJECT_REDEMPTIONS_PATH, ids, token)
            .await
    }

    pub async fn list_pending_redemptions(
        &self,
        token: &str,
    ) -> Result<Vec<RedemptionRecord>, PlatformError> {
        let request = authorized(
            GatewayRequest::get(REDEMPTIONS_PATH),
            token,
            REDEMPTIONS_PATH,
        )
        .query("status", PENDING_STATUS);
        let response = self.send(request).await?;
        let envelope: RedemptionsEnvelope = response.json().map_err(malformed)?;

        Ok(envelope
            .data
            .into_iter()
            .map(|d| RedemptionRecord {
                id: d.id,
                reward_id: d.reward.id,
                reward_title: d.reward.title,
                redeemer_user_id: d.redeemer.user_id,
                redeemer_username: d.redeemer.username,
                user_input: d.user_input,
            })
            .collect())
    }

    async fn post_redemption_batch(
        &self,
        path: &'static str,
        ids: &[String],
        token: &str,
    ) -> Result<(), PlatformError> {
        let request = authorized(GatewayRequest::post(path), token, path)
            .body(json_body(&RedemptionBatchBody { ids })?);
        self.send(request).await.map(drop)
    }

    async fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, PlatformError> {
        self.api
            .send(request)
            .await
            .map_err(|failure| platform_error(failure, REWARDS_REFUSALS))
    }
}

fn reward_path(reward_id: &str) -> String {
    format!("{REWARDS_PATH}/{reward_id}")
}

fn check_redemption_batch(operation: &str, ids: &[String]) -> Result<(), PlatformError> {
    if ids.is_empty() {
        return Err(PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: format!("{operation}: ids list is empty"),
        });
    }
    if ids.len() > MAX_REDEMPTION_BATCH {
        return Err(PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: format!(
                "{operation}: {} ids exceeds the maximum of {MAX_REDEMPTION_BATCH}",
                ids.len()
            ),
        });
    }
    Ok(())
}
