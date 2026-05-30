use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::chat_poller::{QuotaState, today_pacific};
use crate::live_chat_id::LiveChatIdHandle;

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const SEND_COST: u32 = 50;

pub struct YoutubeSendChat {
    client: reqwest::Client,
    access_token_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    live_chat_id: LiveChatIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeSendChat {
    pub fn new(
        access_token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        live_chat_id: LiveChatIdHandle,
        quota: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            live_chat_id,
            quota,
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn send(&self, body: &str) -> Result<(), PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "send chat — no active YouTube broadcast".to_owned(),
            })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(SEND_COST, today)?;
        }

        let token = (self.access_token_source)().await?;

        let url = format!("{}/liveChat/messages", self.api_base);
        let payload = serde_json::json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "type": "textMessageEvent",
                "textMessageDetails": { "messageText": body }
            }
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet")])
            .json(&payload)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 201 {
            return Ok(());
        }

        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(30);

        let body_text = resp.text().await.unwrap_or_default();

        match status {
            429 => Err(PlatformError::RateLimited { retry_after_secs }),
            403 if body_text.contains("operationNotSupported") => Err(PlatformError::Auth {
                reason: "chat write scope missing".to_owned(),
            }),
            _ => Err(PlatformError::Http {
                status,
                body: body_text,
            }),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use forge_platform_core::PlatformError;
    use futures::future::BoxFuture;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn make_sender(server: &MockServer) -> (YoutubeSendChat, LiveChatIdHandle) {
        let handle = LiveChatIdHandle::new();
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let sender =
            YoutubeSendChat::new(token_source(), handle.clone(), quota).with_api_base(server.uri());
        (sender, handle)
    }

    fn make_sender_with_quota(
        server: &MockServer,
        quota: Arc<Mutex<QuotaState>>,
    ) -> (YoutubeSendChat, LiveChatIdHandle) {
        let handle = LiveChatIdHandle::new();
        let sender =
            YoutubeSendChat::new(token_source(), handle.clone(), quota).with_api_base(server.uri());
        (sender, handle)
    }

    #[tokio::test]
    async fn send_returns_unsupported_when_no_live_chat_id() {
        let server = MockServer::start().await;
        let (sender, _handle) = make_sender(&server);

        let err = sender.send("hello").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported when no active broadcast, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_posts_correct_body_when_active() {
        let server = MockServer::start().await;

        let expected_body = json!({
            "snippet": {
                "liveChatId": "lc-test-abc",
                "type": "textMessageEvent",
                "textMessageDetails": { "messageText": "hello chat" }
            }
        });

        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .and(body_json(expected_body))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"kind": "youtube#liveChatMessage"})),
            )
            .mount(&server)
            .await;

        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let (sender, handle) = make_sender_with_quota(&server, Arc::clone(&quota));
        handle.set(Some("lc-test-abc".to_owned()));

        sender.send("hello chat").await.unwrap();

        let qt = quota.lock().await;
        assert_eq!(
            qt.used_today, SEND_COST,
            "quota must be charged {SEND_COST} units"
        );
    }

    #[tokio::test]
    async fn send_returns_quota_exhausted_at_limit() {
        let server = MockServer::start().await;
        let handle = LiveChatIdHandle::new();
        handle.set(Some("lc-test".to_owned()));

        let qt_arc = Arc::new(Mutex::new(QuotaState::default()));
        let today = today_pacific();
        {
            let mut qt = qt_arc.lock().await;
            qt.used_today = 9999;
            qt.peak_seen = 9999;
            qt.last_reset_date = today;
        }
        let sender_at_limit = YoutubeSendChat::new(token_source(), handle.clone(), qt_arc)
            .with_api_base(server.uri());
        let err = sender_at_limit.send("hi").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::QuotaExhausted),
            "expected QuotaExhausted at limit, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_maps_403_forbidden_to_http_403() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "error": {
                    "code": 403,
                    "errors": [{ "reason": "forbidden" }]
                }
            })))
            .mount(&server)
            .await;

        let (sender, handle) = make_sender(&server);
        handle.set(Some("lc-forbidden".to_owned()));

        let err = sender.send("blocked").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 403, .. }),
            "expected Http 403 for forbidden, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_maps_403_operation_not_supported_to_auth_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "error": {
                    "code": 403,
                    "errors": [{ "reason": "operationNotSupported" }]
                }
            })))
            .mount(&server)
            .await;

        let (sender, handle) = make_sender(&server);
        handle.set(Some("lc-nosupport".to_owned()));

        let err = sender.send("blocked").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { .. }),
            "expected Auth for operationNotSupported, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_respects_authorization_bearer_header() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let (sender, handle) = make_sender(&server);
        handle.set(Some("lc-auth-check".to_owned()));

        sender.send("hello").await.unwrap();
    }

    #[tokio::test]
    async fn send_maps_429_to_rate_limited() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "60")
                    .set_body_string("too many requests"),
            )
            .mount(&server)
            .await;

        let (sender, handle) = make_sender(&server);
        handle.set(Some("lc-ratelimit".to_owned()));

        let err = sender.send("hi").await.unwrap_err();
        assert!(
            matches!(
                err,
                PlatformError::RateLimited {
                    retry_after_secs: 60
                }
            ),
            "expected RateLimited with retry_after_secs=60, got: {err}"
        );
    }

    #[tokio::test]
    async fn send_uses_default_retry_after_when_header_absent() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(429).set_body_string("too many requests"))
            .mount(&server)
            .await;

        let (sender, handle) = make_sender(&server);
        handle.set(Some("lc-ratelimit-no-header".to_owned()));

        let err = sender.send("hi").await.unwrap_err();
        assert!(
            matches!(
                err,
                PlatformError::RateLimited {
                    retry_after_secs: 30
                }
            ),
            "expected RateLimited with default retry_after_secs=30, got: {err}"
        );
    }
}
