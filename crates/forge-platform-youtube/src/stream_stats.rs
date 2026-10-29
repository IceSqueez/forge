use std::sync::Arc;

use forge_net::{GatewayRequest, Secret};
use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::data_api::{YoutubeDataApi, malformed};
use crate::timestamp;

const FETCH_COST: u32 = 1;
const VIDEOS_PATH: &str = "/videos";
const MISSING_SCOPE: &str = "stream stats scope missing";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeStreamDetails {
    pub concurrent_viewers: i64,
    pub live_chat_id: String,
    pub actual_start: Option<OffsetDateTime>,
    pub scheduled_start: Option<OffsetDateTime>,
}

pub struct YoutubeStreamStats {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
}

impl YoutubeStreamStats {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
    ) -> Self {
        Self {
            api,
            access_token_source,
            active_broadcast_id,
        }
    }

    pub async fn fetch(&self) -> Result<YoutubeStreamDetails, PlatformError> {
        let broadcast_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "stream stats - no active YouTube broadcast".to_owned(),
                })?;

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::get(VIDEOS_PATH)
            .label(VIDEOS_PATH)
            .bearer(Secret::new(token))
            .query("part", "liveStreamingDetails")
            .query("id", broadcast_id);
        let body: serde_json::Value = self
            .api
            .send(FETCH_COST, request)
            .await
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))?
            .json()
            .map_err(malformed)?;

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
            .and_then(timestamp::parse_logged);
        let scheduled_start_time = details
            .get("scheduledStartTime")
            .and_then(|v| v.as_str())
            .and_then(timestamp::parse_logged);
        let active_live_chat_id = details
            .get("activeLiveChatId")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        Ok(YoutubeStreamDetails {
            concurrent_viewers,
            live_chat_id: active_live_chat_id.to_owned(),
            actual_start: actual_start_time,
            scheduled_start: scheduled_start_time,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::endpoints_test_support::data_api_at;
    use crate::timestamp::tests::instant;

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok("yt-stats-token".to_owned()) }))
    }

    fn stats_on(server: &MockServer, broadcast: Option<&str>) -> YoutubeStreamStats {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        YoutubeStreamStats::new(data_api_at(&server.uri()), token_source(), handle)
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
            Some(&Variant::Datetime(instant("2026-07-24T10:00:00Z")))
        );
        assert_eq!(
            map.get("scheduled_start_time"),
            Some(&Variant::Datetime(instant("2026-07-24T09:30:00Z")))
        );
        assert_eq!(
            map.get("live_chat_id"),
            Some(&Variant::String("lc-abc".to_owned()))
        );
    }

    #[tokio::test]
    async fn fetch_omits_each_start_time_independently_when_absent_or_unreadable() {
        let actual = Variant::Datetime(instant("2026-07-24T10:00:00Z"));
        let scheduled = Variant::Datetime(instant("2026-07-24T09:30:00Z"));
        for (details, expected_actual, expected_scheduled, what) in [
            (
                json!({ "scheduledStartTime": "2026-07-24T09:30:00Z" }),
                None,
                Some(&scheduled),
                "upcoming broadcast without actualStartTime",
            ),
            (
                json!({ "actualStartTime": "2026-07-24T10:00:00Z" }),
                Some(&actual),
                None,
                "unscheduled broadcast without scheduledStartTime",
            ),
            (json!({}), None, None, "both absent"),
            (
                json!({ "actualStartTime": "", "scheduledStartTime": "soon" }),
                None,
                None,
                "empty and garbage text",
            ),
            (
                json!({ "actualStartTime": 1_784_887_200, "scheduledStartTime": null }),
                None,
                None,
                "number and null",
            ),
        ] {
            let mut details = details;
            details["activeLiveChatId"] = json!("lc-abc");
            details["concurrentViewers"] = json!("7");
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/videos"))
                .respond_with(ResponseTemplate::new(200).set_body_json(details_body(details)))
                .mount(&server)
                .await;

            let result = stats_on(&server, Some("bc")).fetch().await.unwrap();
            let Variant::Object(map) = result else {
                panic!("{what}: expected Object, got {result:?}");
            };
            assert_eq!(map.get("actual_start_time"), expected_actual, "{what}");
            assert_eq!(
                map.get("scheduled_start_time"),
                expected_scheduled,
                "{what}"
            );
            assert_eq!(
                map.get("live_chat_id"),
                Some(&Variant::String("lc-abc".to_owned())),
                "{what}"
            );
            assert_eq!(
                map.get("concurrent_viewers"),
                Some(&Variant::Int(7)),
                "{what}"
            );
        }
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
}
