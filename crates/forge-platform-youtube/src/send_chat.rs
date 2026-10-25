use std::sync::Arc;

use forge_net::{GatewayRequest, Secret};
use forge_platform_core::PlatformError;
use futures::future::BoxFuture;

use crate::data_api::{YoutubeDataApi, json_body};
use crate::live_chat_id::LiveChatIdHandle;

const SEND_COST: u32 = 50;
const DELETE_COST: u32 = 50;
const POLL_COST: u32 = 50;
const MESSAGES_PATH: &str = "/liveChat/messages";
const MISSING_SCOPE: &str = "chat write scope missing";

pub struct YoutubeSendChat {
    api: YoutubeDataApi,
    access_token_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    live_chat_id: LiveChatIdHandle,
}

impl YoutubeSendChat {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        live_chat_id: LiveChatIdHandle,
    ) -> Self {
        Self {
            api,
            access_token_source,
            live_chat_id,
        }
    }

    pub async fn delete(&self, message_id: &str) -> Result<(), PlatformError> {
        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::delete(MESSAGES_PATH)
            .label(MESSAGES_PATH)
            .bearer(Secret::new(token))
            .query("id", message_id);
        self.dispatch(DELETE_COST, request).await
    }

    pub async fn send(&self, body: &str) -> Result<(), PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "send chat - no active YouTube broadcast".to_owned(),
            })?;

        let payload = serde_json::json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "textMessageEvent",
                "textMessageDetails": { "messageText": body }
            }
        });
        self.insert(SEND_COST, &payload).await
    }

    pub async fn create_poll(
        &self,
        question: &str,
        options: &[String],
    ) -> Result<(), PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "create poll - no active YouTube broadcast".to_owned(),
            })?;

        let option_objects: Vec<serde_json::Value> = options
            .iter()
            .map(|o| serde_json::json!({ "optionText": o }))
            .collect();
        let payload = serde_json::json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "pollEvent",
                "pollDetails": {
                    "metadata": {
                        "questionText": question,
                        "options": option_objects,
                    }
                }
            }
        });
        self.insert(POLL_COST, &payload).await
    }

    async fn insert(&self, cost: u32, payload: &serde_json::Value) -> Result<(), PlatformError> {
        let body = json_body(payload)?;
        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::post(MESSAGES_PATH)
            .label(MESSAGES_PATH)
            .bearer(Secret::new(token))
            .query("part", "snippet")
            .body(body);
        self.dispatch(cost, request).await
    }

    async fn dispatch(&self, cost: u32, request: GatewayRequest) -> Result<(), PlatformError> {
        self.api
            .send(cost, request)
            .await
            .map(drop)
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::endpoints_test_support::{data_api_charging, unreachable_data_api, used_today};
    use crate::quota_state::SharedQuota;
    use forge_platform_core::PlatformError;
    use futures::future::BoxFuture;
    use reqwest::StatusCode;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn live_chat(id: Option<&str>) -> LiveChatIdHandle {
        let handle = LiveChatIdHandle::new();
        handle.set(id.map(ToOwned::to_owned));
        handle
    }

    fn sender_on(server: &MockServer, quota: &SharedQuota) -> YoutubeSendChat {
        YoutubeSendChat::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            live_chat(Some("lc-test")),
        )
    }

    fn google_refusal(reason: &str) -> ResponseTemplate {
        let forbidden = StatusCode::FORBIDDEN.as_u16();
        ResponseTemplate::new(forbidden).set_body_json(json!({
            "error": { "code": forbidden, "errors": [{ "reason": reason }] }
        }))
    }

    #[tokio::test]
    async fn send_and_create_poll_are_unsupported_without_a_live_chat() {
        let sender = YoutubeSendChat::new(unreachable_data_api(), token_source(), live_chat(None));

        let send = sender.send("hello").await.unwrap_err();
        let poll = sender
            .create_poll("Q?", &["A".to_owned(), "B".to_owned()])
            .await
            .unwrap_err();

        assert!(
            matches!(
                (&send, &poll),
                (
                    PlatformError::Unsupported { .. },
                    PlatformError::Unsupported { .. }
                )
            ),
            "{send} / {poll}"
        );
    }

    #[tokio::test]
    async fn send_posts_a_text_message_to_the_live_chat_and_charges_the_insert_cost() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .and(query_param("part", "snippet"))
            .and(body_json(json!({
                "snippet": {
                    "liveChatId": "lc-test",
                    "type": "textMessageEvent",
                    "textMessageDetails": { "messageText": "hello chat" }
                }
            })))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(json!({"kind": "x"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let quota = SharedQuota::default();

        sender_on(&server, &quota).send("hello chat").await.unwrap();

        assert_eq!(used_today(&quota).await, SEND_COST);
    }

    #[tokio::test]
    async fn send_accepts_any_success_status() {
        for status in [StatusCode::OK, StatusCode::CREATED, StatusCode::NO_CONTENT] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/liveChat/messages"))
                .respond_with(ResponseTemplate::new(status.as_u16()))
                .mount(&server)
                .await;

            let result = sender_on(&server, &SharedQuota::default()).send("hi").await;

            assert!(result.is_ok(), "{status}: {result:?}");
        }
    }

    #[tokio::test]
    async fn send_maps_a_google_quota_refusal_to_quota_exhausted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(google_refusal("quotaExceeded"))
            .mount(&server)
            .await;

        let err = sender_on(&server, &SharedQuota::default())
            .send("hi")
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::QuotaExhausted), "{err}");
    }

    #[tokio::test]
    async fn send_maps_a_missing_permission_refusal_to_auth_naming_the_chat_write_scope() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(google_refusal("insufficientPermissions"))
            .mount(&server)
            .await;

        let err = sender_on(&server, &SharedQuota::default())
            .send("hi")
            .await
            .unwrap_err();

        assert!(
            matches!(&err, PlatformError::Auth { reason } if reason == MISSING_SCOPE),
            "{err}"
        );
    }

    #[tokio::test]
    async fn delete_issues_delete_with_id_query_and_charges_delete_cost() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/messages"))
            .and(query_param("id", "msg-xyz"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let quota = SharedQuota::default();

        sender_on(&server, &quota).delete("msg-xyz").await.unwrap();

        assert_eq!(used_today(&quota).await, DELETE_COST);
    }

    #[tokio::test]
    async fn create_poll_posts_poll_event_body_and_charges_quota() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .and(body_json(json!({
                "snippet": {
                    "liveChatId": "lc-test",
                    "type": "pollEvent",
                    "pollDetails": {
                        "metadata": {
                            "questionText": "Best pick?",
                            "options": [
                                { "optionText": "Red" },
                                { "optionText": "Blue" }
                            ]
                        }
                    }
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;
        let quota = SharedQuota::default();

        sender_on(&server, &quota)
            .create_poll("Best pick?", &["Red".to_owned(), "Blue".to_owned()])
            .await
            .unwrap();

        assert_eq!(used_today(&quota).await, POLL_COST);
    }
}
