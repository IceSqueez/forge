use std::fmt;
use std::sync::Arc;

use forge_net::GatewayRequest;
use forge_platform_core::PlatformError;
use forge_types::Secret;
use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use crate::chat_poller::MISSING_SCOPE;
use crate::data_api::{DataApiError, YoutubeDataApi, malformed};
use crate::quota_state::BROADCAST_COST;

const BROADCASTS_PATH: &str = "/liveBroadcasts";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub(crate) struct ActiveBroadcast {
    pub(crate) live_chat_id: String,
    pub(crate) broadcast_id: String,
    pub(crate) title: String,
}

pub(crate) enum LookupFailure {
    Api(DataApiError),
    Malformed(PlatformError),
}

pub(crate) async fn fetch_active_broadcast(
    api: &YoutubeDataApi,
    token: &str,
    cancel: Option<&CancellationToken>,
) -> Result<Option<ActiveBroadcast>, LookupFailure> {
    let mut request = GatewayRequest::get(BROADCASTS_PATH)
        .label(BROADCASTS_PATH)
        .bearer(Secret::new(token))
        .query("part", "snippet,contentDetails")
        .query("broadcastStatus", "active")
        .query("broadcastType", "all");
    if let Some(cancel) = cancel {
        request = request.cancel_on(cancel.clone());
    }
    let body: serde_json::Value = api
        .send(BROADCAST_COST, request)
        .await
        .map_err(LookupFailure::Api)?
        .json()
        .map_err(|error| LookupFailure::Malformed(malformed(error)))?;
    Ok(active_broadcast_of(&body))
}

fn active_broadcast_of(body: &serde_json::Value) -> Option<ActiveBroadcast> {
    let item = body.get("items")?.as_array()?.first()?;
    let snippet = item.get("snippet")?;
    Some(ActiveBroadcast {
        live_chat_id: snippet.get("liveChatId")?.as_str()?.to_owned(),
        broadcast_id: item.get("id")?.as_str()?.to_owned(),
        title: snippet
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned(),
    })
}

#[derive(Clone)]
pub(crate) struct BroadcastLookup {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
}

impl BroadcastLookup {
    pub(crate) fn new(api: YoutubeDataApi, access_token_source: TokenSource) -> Self {
        Self {
            api,
            access_token_source,
        }
    }

    pub(crate) async fn active(&self) -> Result<Option<ActiveBroadcast>, PlatformError> {
        let token = (self.access_token_source)().await?;
        fetch_active_broadcast(&self.api, &token, None)
            .await
            .map_err(|failure| match failure {
                LookupFailure::Api(error) => error.into_platform_error(MISSING_SCOPE),
                LookupFailure::Malformed(error) => error,
            })
    }
}

impl fmt::Debug for BroadcastLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BroadcastLookup")
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::endpoints_test_support::{data_api_charging, unreachable_data_api, used_today};
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::quota_state::SharedQuota;
    use crate::send_chat::YoutubeSendChat;

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn live_broadcast() -> serde_json::Value {
        json!({
            "items": [{
                "id": "broadcast-1",
                "snippet": { "liveChatId": "chat-1", "title": "Live" }
            }]
        })
    }

    async fn answering(reply: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BROADCASTS_PATH))
            .and(query_param("broadcastStatus", "active"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(reply)
            .mount(&server)
            .await;
        server
    }

    fn lookup_on(server: &MockServer, quota: &SharedQuota) -> BroadcastLookup {
        BroadcastLookup::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
        )
    }

    fn chat_handle(lookup: BroadcastLookup) -> LiveChatIdHandle {
        let handle = LiveChatIdHandle::new();
        handle.fall_back_to(lookup);
        handle
    }

    fn broadcast_handle(lookup: BroadcastLookup) -> ActiveBroadcastIdHandle {
        let handle = ActiveBroadcastIdHandle::new();
        handle.fall_back_to(lookup);
        handle
    }

    #[tokio::test]
    async fn an_empty_cell_resolves_through_one_charged_lookup_to_each_handles_own_id() {
        let server = answering(ResponseTemplate::new(200).set_body_json(live_broadcast())).await;
        let quota = SharedQuota::default();

        let chat = chat_handle(lookup_on(&server, &quota)).resolve().await;
        let broadcast = broadcast_handle(lookup_on(&server, &quota)).resolve().await;

        assert_eq!(chat.unwrap().as_deref(), Some("chat-1"));
        assert_eq!(broadcast.unwrap().as_deref(), Some("broadcast-1"));
        assert_eq!(used_today(&quota).await, 2 * BROADCAST_COST);
    }

    #[tokio::test]
    async fn a_filled_cell_answers_without_a_request_or_a_charge() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let quota = SharedQuota::default();
        let chat = chat_handle(lookup_on(&server, &quota));
        let broadcast = broadcast_handle(lookup_on(&server, &quota));
        chat.set(Some("cached-chat".to_owned()));
        broadcast.set(Some("cached-broadcast".to_owned()));

        let resolved = (
            chat.resolve().await.unwrap(),
            broadcast.resolve().await.unwrap(),
        );

        assert_eq!(
            resolved,
            (
                Some("cached-chat".to_owned()),
                Some("cached-broadcast".to_owned())
            )
        );
        assert_eq!(used_today(&quota).await, 0);
    }

    #[tokio::test]
    async fn an_empty_cell_without_a_lookup_resolves_to_nothing() {
        assert_eq!(LiveChatIdHandle::new().resolve().await.unwrap(), None);
        assert_eq!(
            ActiveBroadcastIdHandle::new().resolve().await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn no_active_broadcast_resolves_to_nothing_and_stays_charged() {
        let server =
            answering(ResponseTemplate::new(200).set_body_json(json!({ "items": [] }))).await;
        let quota = SharedQuota::default();

        let resolved = chat_handle(lookup_on(&server, &quota)).resolve().await;

        assert_eq!(resolved.unwrap(), None);
        assert_eq!(used_today(&quota).await, BROADCAST_COST);
    }

    fn google_refusal(reason: &str) -> ResponseTemplate {
        let forbidden = StatusCode::FORBIDDEN.as_u16();
        ResponseTemplate::new(forbidden).set_body_json(json!({
            "error": { "code": forbidden, "errors": [{ "reason": reason }] }
        }))
    }

    #[tokio::test]
    async fn a_failed_lookup_surfaces_a_typed_error_and_stays_charged() {
        const INTERNAL: u16 = StatusCode::INTERNAL_SERVER_ERROR.as_u16();
        for (label, reply, expected) in [
            (
                "server error",
                ResponseTemplate::new(INTERNAL),
                (|e| {
                    matches!(
                        e,
                        PlatformError::Http {
                            status: INTERNAL,
                            ..
                        }
                    )
                }) as fn(&PlatformError) -> bool,
            ),
            ("quota refusal", google_refusal("quotaExceeded"), |e| {
                matches!(e, PlatformError::QuotaExhausted)
            }),
            (
                "missing scope",
                google_refusal("insufficientPermissions"),
                |e| matches!(e, PlatformError::Auth { reason } if reason == MISSING_SCOPE),
            ),
            (
                "unreadable body",
                ResponseTemplate::new(200).set_body_string("not json"),
                |e| matches!(e, PlatformError::MalformedResponse { .. }),
            ),
        ] {
            let server = answering(reply).await;
            let quota = SharedQuota::default();

            let error = chat_handle(lookup_on(&server, &quota))
                .resolve()
                .await
                .unwrap_err();

            assert!(expected(&error), "{label}: {error:?}");
            assert_eq!(used_today(&quota).await, BROADCAST_COST, "{label}");
        }
    }

    #[tokio::test]
    async fn a_token_failure_stops_the_lookup_before_any_charge() {
        let quota = SharedQuota::default();
        let failing: TokenSource = Arc::new(|| {
            Box::pin(async {
                Err(PlatformError::Auth {
                    reason: "signed out".to_owned(),
                })
            })
        });
        let lookup = BroadcastLookup::new(
            data_api_charging("http://127.0.0.1:0", quota.clone()),
            failing,
        );

        let error = chat_handle(lookup).resolve().await.unwrap_err();

        assert!(matches!(error, PlatformError::Auth { .. }), "{error:?}");
        assert_eq!(used_today(&quota).await, 0);
    }

    #[test]
    fn only_an_item_with_both_ids_counts_as_an_active_broadcast() {
        let item = |value: serde_json::Value| json!({ "items": [value] });
        for (label, body, found) in [
            ("no items key", json!({}), false),
            ("empty items", json!({ "items": [] }), false),
            ("no snippet", item(json!({ "id": "b" })), false),
            (
                "no live chat",
                item(json!({ "id": "b", "snippet": { "title": "t" } })),
                false,
            ),
            (
                "no id",
                item(json!({ "snippet": { "liveChatId": "c" } })),
                false,
            ),
            (
                "no title",
                item(json!({ "id": "b", "snippet": { "liveChatId": "c" } })),
                true,
            ),
        ] {
            assert_eq!(active_broadcast_of(&body).is_some(), found, "{label}");
        }
    }

    #[tokio::test]
    async fn sending_with_an_empty_cell_posts_to_the_looked_up_live_chat() {
        let server = answering(ResponseTemplate::new(200).set_body_json(live_broadcast())).await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .and(wiremock::matchers::body_partial_json(
                json!({ "snippet": { "liveChatId": "chat-1" } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        let quota = SharedQuota::default();
        let sender = YoutubeSendChat::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            chat_handle(lookup_on(&server, &quota)),
        );

        sender.send("hello").await.unwrap();

        server.verify().await;
    }

    #[tokio::test]
    async fn sending_with_no_active_broadcast_stays_unsupported_and_posts_nothing() {
        let server =
            answering(ResponseTemplate::new(200).set_body_json(json!({ "items": [] }))).await;
        let quota = SharedQuota::default();
        let sender = YoutubeSendChat::new(
            unreachable_data_api(),
            token_source(),
            chat_handle(lookup_on(&server, &quota)),
        );

        let error = sender.send("hello").await.unwrap_err();

        assert!(
            matches!(error, PlatformError::Unsupported { .. }),
            "{error:?}"
        );
    }
}
