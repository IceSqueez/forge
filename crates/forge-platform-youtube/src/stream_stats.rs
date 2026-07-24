use std::collections::BTreeMap;
use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_types::Variant;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const FETCH_COST: u32 = 1;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeStreamStats {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeStreamStats {
    pub fn new(
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
        quota: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            active_broadcast_id,
            quota,
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn fetch(&self) -> Result<Variant, PlatformError> {
        let broadcast_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "stream stats - no active YouTube broadcast".to_owned(),
                })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(FETCH_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/videos", self.api_base);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[
                ("part", "liveStreamingDetails"),
                ("id", broadcast_id.as_str()),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status != 200 {
            return Err(self.map_failure(resp).await);
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

        let details = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("liveStreamingDetails"))
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "stream stats - active broadcast video not found".to_owned(),
            })?;

        let concurrent_viewers = details
            .get("concurrentViewers")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let actual_start_time = details
            .get("actualStartTime")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let scheduled_start_time = details
            .get("scheduledStartTime")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let active_live_chat_id = details
            .get("activeLiveChatId")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        Ok(Variant::Object(BTreeMap::from([
            (
                "concurrent_viewers".to_owned(),
                Variant::Int(concurrent_viewers),
            ),
            (
                "actual_start_time".to_owned(),
                Variant::String(actual_start_time.to_owned()),
            ),
            (
                "scheduled_start_time".to_owned(),
                Variant::String(scheduled_start_time.to_owned()),
            ),
            (
                "live_chat_id".to_owned(),
                Variant::String(active_live_chat_id.to_owned()),
            ),
        ])))
    }

    async fn map_failure(&self, resp: reqwest::Response) -> PlatformError {
        let status = resp.status().as_u16();
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(30);
        let body_text = resp.text().await.unwrap_or_default();

        match status {
            429 => PlatformError::RateLimited { retry_after_secs },
            403 if body_text.contains("quotaExceeded") => PlatformError::QuotaExhausted,
            403 if body_text.contains("insufficientPermissions")
                || body_text.contains("operationNotSupported") =>
            {
                PlatformError::Auth {
                    reason: "stream stats scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-stats-secret-token";

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn stats_on(server: &MockServer, broadcast: Option<&str>) -> YoutubeStreamStats {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        YoutubeStreamStats::new(token_source(), handle, quota).with_api_base(server.uri())
    }

    fn details_body(extra: serde_json::Value) -> serde_json::Value {
        json!({ "items": [{ "liveStreamingDetails": extra }] })
    }

    #[tokio::test]
    async fn fetch_returns_unsupported_when_no_active_broadcast() {
        let server = MockServer::start().await;
        let err = stats_on(&server, None).fetch().await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported without a broadcast, got: {err}"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_returns_live_details_variant_with_query_contract() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .and(query_param("part", "liveStreamingDetails"))
            .and(query_param("id", "bc-live"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(details_body(json!({
                    "concurrentViewers": "1234",
                    "actualStartTime": "2026-07-24T10:00:00Z",
                    "scheduledStartTime": "2026-07-24T09:30:00Z",
                    "activeLiveChatId": "lc-abc"
                }))),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = stats_on(&server, Some("bc-live")).fetch().await.unwrap();
        let Variant::Object(map) = result else {
            panic!("expected Object, got {result:?}");
        };
        assert_eq!(map.get("concurrent_viewers"), Some(&Variant::Int(1234)));
        assert_eq!(
            map.get("actual_start_time"),
            Some(&Variant::String("2026-07-24T10:00:00Z".to_owned()))
        );
        assert_eq!(
            map.get("scheduled_start_time"),
            Some(&Variant::String("2026-07-24T09:30:00Z".to_owned()))
        );
        assert_eq!(
            map.get("live_chat_id"),
            Some(&Variant::String("lc-abc".to_owned()))
        );
    }

    #[tokio::test]
    async fn fetch_defaults_concurrent_viewers_to_zero_when_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(details_body(json!({
                    "actualStartTime": "2026-07-24T10:00:00Z"
                }))),
            )
            .mount(&server)
            .await;

        let result = stats_on(&server, Some("bc")).fetch().await.unwrap();
        let Variant::Object(map) = result else {
            panic!("expected Object, got {result:?}");
        };
        assert_eq!(map.get("concurrent_viewers"), Some(&Variant::Int(0)));
    }

    #[tokio::test]
    async fn fetch_returns_unsupported_when_details_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
            .mount(&server)
            .await;

        let err = stats_on(&server, Some("bc")).fetch().await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "missing liveStreamingDetails must map to Unsupported, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_maps_500_to_http() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream failure"))
            .mount(&server)
            .await;

        let err = stats_on(&server, Some("bc")).fetch().await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 500, .. }),
            "500 must map to Http 500, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_error_does_not_leak_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let err = stats_on(&server, Some("bc")).fetch().await.unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "error must not leak the bearer token: {msg}"
        );
        assert!(
            !msg.contains(&server.uri()),
            "error must not leak the request URL: {msg}"
        );
    }
}
