use forge_net::{Gateway, GatewayRequest, GatewayResponse};
use forge_platform_core::PlatformError;
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
        match response.json::<CreateResponse>() {
            Ok(created) => Ok(created.id.or_else(|| created.data.and_then(|d| d.id))),
            Err(error) => {
                tracing::debug!(%error, "kick reward create response carried no readable id");
                Ok(None)
            }
        }
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
        return Err(PlatformError::Refused {
            reason: format!("{operation}: ids list is empty"),
        });
    }
    if ids.len() > MAX_REDEMPTION_BATCH {
        return Err(PlatformError::Refused {
            reason: format!(
                "{operation}: {} ids exceeds the maximum of {MAX_REDEMPTION_BATCH}",
                ids.len()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn rewards_on(server: &MockServer) -> KickRewards {
        KickRewards::new(crate::endpoints_test_support::api_at(&server.uri()))
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

        assert!(matches!(err, PlatformError::Refused { .. }));
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

        assert!(matches!(err, PlatformError::Refused { .. }));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "the oversize-batch guard must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn list_pending_redemptions_queries_pending_status_and_accepts_empty_data() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .and(query_param("status", "pending"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let records = rewards_on(&server)
            .list_pending_redemptions("tok")
            .await
            .unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn list_pending_redemptions_maps_each_nested_path_onto_records() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {
                        "id": "rd_1",
                        "reward": { "id": "rw_1", "title": "Hydrate" },
                        "redeemer": { "user_id": 100, "username": "alice" },
                        "user_input": "stay healthy"
                    },
                    {
                        "id": "rd_2",
                        "reward": { "id": "rw_2", "title": "Stretch" },
                        "redeemer": { "user_id": 200, "username": "bob" },
                        "user_input": "do it"
                    }
                ]
            })))
            .mount(&server)
            .await;

        let records = rewards_on(&server)
            .list_pending_redemptions("tok")
            .await
            .unwrap();
        assert_eq!(records.len(), 2);

        assert_eq!(records[0].id, "rd_1");
        assert_eq!(records[0].reward_id, "rw_1");
        assert_eq!(records[0].reward_title, "Hydrate");
        assert_eq!(records[0].redeemer_user_id, 100);
        assert_eq!(records[0].redeemer_username, "alice");
        assert_eq!(records[0].user_input, "stay healthy");

        assert_eq!(records[1].id, "rd_2");
        assert_eq!(records[1].reward_id, "rw_2");
        assert_eq!(records[1].reward_title, "Stretch");
        assert_eq!(records[1].redeemer_user_id, 200);
        assert_eq!(records[1].redeemer_username, "bob");
        assert_eq!(records[1].user_input, "do it");
    }

    #[tokio::test]
    async fn list_pending_redemptions_missing_redeemer_defaults_while_reward_stays_populated() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "rd_1",
                    "reward": { "id": "rw_1", "title": "Hydrate" },
                    "user_input": "no redeemer block"
                }]
            })))
            .mount(&server)
            .await;

        let records = rewards_on(&server)
            .list_pending_redemptions("tok")
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].redeemer_user_id, 0);
        assert_eq!(records[0].redeemer_username, "");
        assert_eq!(records[0].reward_id, "rw_1");
        assert_eq!(records[0].reward_title, "Hydrate");
    }

    #[tokio::test]
    async fn create_keeps_a_success_with_an_undecodable_body_as_created_without_id() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(ResponseTemplate::new(201).set_body_string("created"))
            .mount(&server)
            .await;

        let id = rewards_on(&server).create(minimal_create(), "tok").await;

        assert_eq!(id.unwrap(), None);
    }

    #[tokio::test]
    async fn list_pending_redemptions_reports_an_undecodable_success_body_as_malformed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[oops"))
            .mount(&server)
            .await;

        let result = rewards_on(&server).list_pending_redemptions("tok").await;

        assert!(
            matches!(result, Err(PlatformError::MalformedResponse { .. })),
            "{:?}",
            result.map(|records| records.len())
        );
    }
}
