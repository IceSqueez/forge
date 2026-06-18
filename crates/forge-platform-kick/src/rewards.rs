use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use serde::{Deserialize, Serialize};

const REWARDS_ENDPOINT: &str = "https://api.kick.com/public/v1/channels/rewards";
const MAX_REDEMPTION_BATCH: usize = 25;

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
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            rewards_endpoint: REWARDS_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
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

    pub async fn accept_redemptions(
        &self,
        ids: &[String],
        token: &str,
    ) -> Result<(), PlatformError> {
        if ids.is_empty() {
            return Err(PlatformError::Http {
                status: 0,
                body: "accept_redemptions: ids list is empty".to_owned(),
            });
        }
        if ids.len() > MAX_REDEMPTION_BATCH {
            return Err(PlatformError::Http {
                status: 0,
                body: format!(
                    "accept_redemptions: {} ids exceeds the maximum of {MAX_REDEMPTION_BATCH}",
                    ids.len()
                ),
            });
        }

        self.acquire_slot().await?;

        let url = format!("{}/redemptions/accept", self.rewards_endpoint);
        let body = RedemptionBatchBody { ids };
        let response = self
            .client
            .post(&url)
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

    pub async fn reject_redemptions(
        &self,
        ids: &[String],
        token: &str,
    ) -> Result<(), PlatformError> {
        if ids.is_empty() {
            return Err(PlatformError::Http {
                status: 0,
                body: "reject_redemptions: ids list is empty".to_owned(),
            });
        }
        if ids.len() > MAX_REDEMPTION_BATCH {
            return Err(PlatformError::Http {
                status: 0,
                body: format!(
                    "reject_redemptions: {} ids exceeds the maximum of {MAX_REDEMPTION_BATCH}",
                    ids.len()
                ),
            });
        }

        self.acquire_slot().await?;

        let url = format!("{}/redemptions/reject", self.rewards_endpoint);
        let body = RedemptionBatchBody { ids };
        let response = self
            .client
            .post(&url)
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

    pub async fn list_pending_redemptions(
        &self,
        token: &str,
    ) -> Result<Vec<RedemptionRecord>, PlatformError> {
        self.acquire_slot().await?;

        let url = format!("{}/redemptions?status=pending", self.rewards_endpoint);
        let response = self
            .client
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return map_rewards_error(status, response).await;
        }

        let envelope: RedemptionsEnvelope =
            response.json().await.map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
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

    fn rewards_on(server: &MockServer) -> KickRewards {
        KickRewards::new(Arc::new(GrantLimiter)).with_api_base(server.uri())
    }

    async fn last_body(server: &MockServer) -> serde_json::Value {
        let reqs = server.received_requests().await.unwrap();
        let body = reqs.last().unwrap().body.clone();
        serde_json::from_slice(&body).unwrap()
    }

    fn minimal_create() -> CreateRewardParams {
        CreateRewardParams {
            title: "Hydrate".to_owned(),
            cost: 500,
            description: None,
            background_color: None,
            is_enabled: None,
            is_user_input_required: None,
            should_redemptions_skip_request_queue: None,
        }
    }

    #[tokio::test]
    async fn create_posts_to_rewards_endpoint_with_only_required_fields() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let result = rewards_on(&server).create(minimal_create(), "tok").await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["title"], "Hydrate");
        assert_eq!(body["cost"], 500);
        for omitted in [
            "description",
            "background_color",
            "is_enabled",
            "is_user_input_required",
            "should_redemptions_skip_request_queue",
        ] {
            assert!(
                body.get(omitted).is_none(),
                "unset optional field {omitted} must be skipped from the body"
            );
        }
    }

    #[tokio::test]
    async fn create_returns_id_from_top_level_id_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({"id": "rw_1"})),
            )
            .mount(&server)
            .await;

        let id = rewards_on(&server).create(minimal_create(), "tok").await;
        assert_eq!(id.unwrap(), Some("rw_1".to_owned()));
    }

    #[tokio::test]
    async fn create_returns_id_from_nested_data_id_when_top_level_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({"data": {"id": "rw_2"}})),
            )
            .mount(&server)
            .await;

        let id = rewards_on(&server).create(minimal_create(), "tok").await;
        assert_eq!(id.unwrap(), Some("rw_2".to_owned()));
    }

    #[tokio::test]
    async fn create_returns_none_when_success_body_carries_no_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let id = rewards_on(&server).create(minimal_create(), "tok").await;
        assert_eq!(id.unwrap(), None);
    }

    #[tokio::test]
    async fn update_patches_reward_by_id_with_only_provided_fields() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/channels/rewards/rw_9"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let params = UpdateRewardParams {
            title: Some("New".to_owned()),
            cost: None,
            description: None,
            background_color: None,
            is_enabled: None,
            is_paused: None,
            is_user_input_required: None,
            should_redemptions_skip_request_queue: None,
        };
        let result = rewards_on(&server).update("rw_9", params, "tok").await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["title"], "New");
        for omitted in ["cost", "description", "is_paused", "is_enabled"] {
            assert!(
                body.get(omitted).is_none(),
                "unset update field {omitted} must be skipped from the body"
            );
        }
    }

    #[tokio::test]
    async fn delete_issues_delete_to_reward_by_id() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/channels/rewards/rw_9"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let result = rewards_on(&server).delete("rw_9", "tok").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn auth_statuses_map_to_auth_error() {
        for status in [401_u16, 403] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;

            let err = rewards_on(&server)
                .create(minimal_create(), "tok")
                .await
                .unwrap_err();
            assert!(
                matches!(err, PlatformError::Auth { .. }),
                "status {status} must map to Auth, got {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn unprocessable_entity_maps_to_http_error_with_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(422))
            .mount(&server)
            .await;

        let err = rewards_on(&server)
            .create(minimal_create(), "tok")
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Http { status: 422, .. }));
    }

    #[tokio::test]
    async fn rate_limited_status_maps_to_rate_limited_with_parsed_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "57"))
            .mount(&server)
            .await;

        let err = rewards_on(&server)
            .create(minimal_create(), "tok")
            .await
            .unwrap_err();
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

        let client = KickRewards::new(Arc::new(ExhaustedLimiter)).with_api_base(server.uri());
        let err = client.create(minimal_create(), "tok").await.unwrap_err();

        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an exhausted limiter must short-circuit before any HTTP call"
        );
    }

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("rd_{i}")).collect()
    }

    #[tokio::test]
    async fn accept_redemptions_posts_ids_array_to_accept_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards/redemptions/accept"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let batch = vec!["rd_a".to_owned(), "rd_b".to_owned()];
        let result = rewards_on(&server).accept_redemptions(&batch, "tok").await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["ids"], serde_json::json!(["rd_a", "rd_b"]));
    }

    #[tokio::test]
    async fn reject_redemptions_posts_ids_array_to_reject_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards/redemptions/reject"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let batch = vec!["rd_a".to_owned(), "rd_b".to_owned()];
        let result = rewards_on(&server).reject_redemptions(&batch, "tok").await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["ids"], serde_json::json!(["rd_a", "rd_b"]));
    }

    #[tokio::test]
    async fn accept_redemptions_empty_ids_errors_without_reaching_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let err = rewards_on(&server)
            .accept_redemptions(&[], "tok")
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::Http { status: 0, .. }));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "the client-side empty guard must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn accept_redemptions_at_batch_limit_reaches_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards/redemptions/accept"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let result = rewards_on(&server)
            .accept_redemptions(&ids(MAX_REDEMPTION_BATCH), "tok")
            .await;
        assert!(result.is_ok(), "{MAX_REDEMPTION_BATCH} ids is at the limit");
    }

    #[tokio::test]
    async fn accept_redemptions_one_over_batch_limit_errors_without_reaching_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let err = rewards_on(&server)
            .accept_redemptions(&ids(MAX_REDEMPTION_BATCH + 1), "tok")
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::Http { status: 0, .. }));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "the oversize-batch guard must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn accept_redemptions_maps_auth_and_rate_limited_statuses() {
        for (status, expect_auth) in [(401_u16, true), (429, false)] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;

            let err = rewards_on(&server)
                .accept_redemptions(&ids(1), "tok")
                .await
                .unwrap_err();

            if expect_auth {
                assert!(matches!(err, PlatformError::Auth { .. }), "status {status}");
            } else {
                assert!(
                    matches!(err, PlatformError::RateLimited { .. }),
                    "status {status}"
                );
            }
        }
    }

    #[tokio::test]
    async fn accept_redemptions_limiter_exhaustion_short_circuits_after_guards() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = KickRewards::new(Arc::new(ExhaustedLimiter)).with_api_base(server.uri());
        let err = client.accept_redemptions(&ids(1), "tok").await.unwrap_err();

        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an exhausted limiter must short-circuit before any HTTP call"
        );
    }
}
