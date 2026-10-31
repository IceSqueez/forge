use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::PlatformError;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::gateway::{RefusalReasons, authorized, json_body, malformed, platform_error};

const CHANNELS_PATH: &str = "/channels";

const CHANNEL_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "channel token rejected (401)",
    forbidden: Some("channel forbidden (403); check channel scope"),
};

pub struct KickChannel {
    api: Gateway,
}

pub struct ChannelSnapshot {
    pub broadcaster_user_id: u64,
    pub slug: String,
    pub is_live: bool,
    pub stream_title: String,
    pub category_id: Option<u64>,
    pub category_name: String,
    pub viewer_count: u64,
    pub started_at: Option<OffsetDateTime>,
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

#[derive(Deserialize)]
struct ChannelsEnvelope {
    #[serde(default)]
    data: Vec<ChannelData>,
}

#[derive(Deserialize, Default)]
struct ChannelData {
    #[serde(default)]
    broadcaster_user_id: Option<u64>,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    stream_title: String,
    #[serde(default)]
    category: CategoryData,
    #[serde(default)]
    stream: StreamData,
}

#[derive(Deserialize, Default)]
struct CategoryData {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Default)]
struct StreamData {
    #[serde(default)]
    is_live: bool,
    #[serde(default)]
    viewer_count: u64,
    #[serde(default)]
    start_time: Option<String>,
}

impl KickChannel {
    pub fn new(api: Gateway) -> Self {
        Self { api }
    }

    pub async fn update_info(
        &self,
        token: &str,
        title: Option<String>,
        category_id: Option<u64>,
        tags: Option<Vec<String>>,
    ) -> Result<(), PlatformError> {
        let body = UpdateBody {
            stream_title: title,
            category_id,
            custom_tags: tags,
        };
        let request = authorized(GatewayRequest::patch(CHANNELS_PATH), token, CHANNELS_PATH)
            .body(json_body(&body)?);
        self.api
            .send(request)
            .await
            .map(drop)
            .map_err(|failure| platform_error(failure, CHANNEL_REFUSALS))
    }

    pub async fn get_channel(&self, token: &str) -> Result<ChannelSnapshot, PlatformError> {
        self.fetch(token, None).await
    }

    pub async fn get_channel_by_slug(
        &self,
        token: &str,
        slug: &str,
    ) -> Result<ChannelSnapshot, PlatformError> {
        self.fetch(token, Some(slug)).await
    }

    async fn fetch(
        &self,
        token: &str,
        slug: Option<&str>,
    ) -> Result<ChannelSnapshot, PlatformError> {
        let mut request = authorized(GatewayRequest::get(CHANNELS_PATH), token, CHANNELS_PATH);
        if let Some(slug) = slug {
            request = request.query("slug", slug);
        }
        let response = self
            .api
            .send(request)
            .await
            .map_err(|failure| platform_error(failure, CHANNEL_REFUSALS))?;
        let envelope: ChannelsEnvelope = response.json().map_err(malformed)?;

        let channel =
            envelope
                .data
                .into_iter()
                .next()
                .ok_or_else(|| PlatformError::MalformedResponse {
                    reason: "channels GET returned no data".to_owned(),
                })?;

        let broadcaster_user_id =
            channel
                .broadcaster_user_id
                .ok_or_else(|| PlatformError::MalformedResponse {
                    reason: "channels GET returned no broadcaster id".to_owned(),
                })?;

        Ok(ChannelSnapshot {
            broadcaster_user_id,
            slug: channel.slug,
            is_live: channel.stream.is_live,
            stream_title: channel.stream_title,
            category_id: channel.category.id,
            category_name: channel.category.name,
            viewer_count: channel.stream.viewer_count,
            started_at: channel
                .stream
                .start_time
                .as_deref()
                .and_then(crate::timestamp::parse_logged),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn channel_on(server: &MockServer) -> KickChannel {
        KickChannel::new(crate::test_support::api_at(&server.uri()))
    }

    async fn last_body(server: &MockServer) -> serde_json::Value {
        let reqs = server.received_requests().await.unwrap();
        let body = reqs.last().unwrap().body.clone();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn partial_update_with_only_title_omits_unset_keys_from_body() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .expect(1)
            .mount(&server)
            .await;

        let result = channel_on(&server)
            .update_info("tok", Some("New Title".to_owned()), None, None)
            .await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["stream_title"], "New Title");
        assert!(
            body.get("category_id").is_none(),
            "unset category_id must be skipped from the body"
        );
        assert!(
            body.get("custom_tags").is_none(),
            "unset tags must be skipped from the body"
        );
    }

    #[tokio::test]
    async fn full_update_carries_title_category_and_tags() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .expect(1)
            .mount(&server)
            .await;

        let result = channel_on(&server)
            .update_info(
                "tok",
                Some("Title".to_owned()),
                Some(42),
                Some(vec!["speedrun".to_owned(), "rust".to_owned()]),
            )
            .await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["stream_title"], "Title");
        assert_eq!(body["category_id"], 42);
        assert_eq!(body["custom_tags"], serde_json::json!(["speedrun", "rust"]));
    }

    #[tokio::test]
    async fn get_channel_maps_first_data_element_onto_snapshot_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                "data": [{
                    "broadcaster_user_id": 42,
                    "slug": "a-streamer",
                    "stream_title": "Speedrun Night",
                    "category": { "id": 77, "name": "Just Chatting" },
                    "stream": { "is_live": true, "viewer_count": 1234, "start_time": "2026-07-24T10:00:00Z" }
                }]
            })))
            .mount(&server)
            .await;

        let snapshot = channel_on(&server).get_channel("tok").await.unwrap();
        assert_eq!(snapshot.broadcaster_user_id, 42);
        assert_eq!(snapshot.slug, "a-streamer");
        assert!(snapshot.is_live);
        assert_eq!(snapshot.stream_title, "Speedrun Night");
        assert_eq!(snapshot.category_id, Some(77));
        assert_eq!(snapshot.category_name, "Just Chatting");
        assert_eq!(snapshot.viewer_count, 1234);
    }

    #[tokio::test]
    async fn stream_start_time_maps_to_started_at_without_dropping_live_stats() {
        for (stream, expected, what) in [
            (
                serde_json::json!({ "is_live": true, "viewer_count": 31, "start_time": null }),
                None,
                "null",
            ),
            (
                serde_json::json!({ "is_live": true, "viewer_count": 31 }),
                None,
                "missing key",
            ),
            (
                serde_json::json!({ "is_live": true, "viewer_count": 31, "start_time": "" }),
                None,
                "empty string",
            ),
            (
                serde_json::json!({
                    "is_live": true,
                    "viewer_count": 31,
                    "start_time": "2026-07-24T10:00:00+02:00"
                }),
                Some(crate::timestamp::tests::instant("2026-07-24T08:00:00Z")),
                "rfc 3339 with offset",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/channels"))
                .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "data": [{ "broadcaster_user_id": 1, "stream_title": "t", "stream": stream }]
                })))
                .mount(&server)
                .await;

            let snapshot = channel_on(&server)
                .get_channel("tok")
                .await
                .unwrap_or_else(|err| panic!("{what}: lookup must succeed, got {err:?}"));
            assert_eq!(snapshot.started_at, expected, "{what}");
            assert!(snapshot.is_live, "{what}");
            assert_eq!(snapshot.viewer_count, 31, "{what}");
        }
    }

    #[tokio::test]
    async fn get_channel_empty_data_maps_to_http_status_zero_no_data_guard() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "data": []
                })),
            )
            .mount(&server)
            .await;

        let result = channel_on(&server).get_channel("tok").await;
        assert!(matches!(
            result,
            Err(PlatformError::MalformedResponse { .. })
        ));
    }

    #[tokio::test]
    async fn get_channel_without_a_broadcaster_id_is_malformed() {
        for (label, entry) in [
            ("key absent", serde_json::json!({ "slug": "a-streamer" })),
            (
                "explicit null",
                serde_json::json!({ "broadcaster_user_id": null, "slug": "a-streamer" }),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/channels"))
                .respond_with(
                    ResponseTemplate::new(reqwest::StatusCode::OK)
                        .set_body_json(serde_json::json!({ "data": [entry] })),
                )
                .mount(&server)
                .await;

            let result = channel_on(&server).get_channel("tok").await;

            assert!(
                matches!(result, Err(PlatformError::MalformedResponse { .. })),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn get_channel_missing_optional_objects_default_to_zero_and_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "data": [{
                        "broadcaster_user_id": 1,
                        "stream_title": "Offline"
                    }]
                })),
            )
            .mount(&server)
            .await;

        let snapshot = channel_on(&server).get_channel("tok").await.unwrap();
        assert_eq!(snapshot.category_id, None);
        assert_eq!(snapshot.category_name, "");
        assert_eq!(snapshot.viewer_count, 0);
        assert!(!snapshot.is_live);
        assert_eq!(snapshot.started_at, None);
    }

    #[tokio::test]
    async fn live_flag_is_read_from_the_nested_stream_object_only() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "data": [{
                        "broadcaster_user_id": 1,
                        "is_live": true,
                        "stream": { "is_live": false, "viewer_count": 0 }
                    }]
                })),
            )
            .mount(&server)
            .await;

        let snapshot = channel_on(&server).get_channel("tok").await.unwrap();
        assert!(!snapshot.is_live);
    }

    #[tokio::test]
    async fn get_channel_by_slug_queries_the_requested_channel() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .and(query_param("slug", "other-streamer"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "data": [{ "broadcaster_user_id": 1, "slug": "other-streamer" }]
                })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let snapshot = channel_on(&server)
            .get_channel_by_slug("tok", "other-streamer")
            .await
            .unwrap();
        assert_eq!(snapshot.slug, "other-streamer");
    }

    #[tokio::test]
    async fn get_channel_reports_an_undecodable_success_body_as_malformed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("not json"),
            )
            .mount(&server)
            .await;

        let result = channel_on(&server).get_channel("tok").await;

        assert!(matches!(
            result,
            Err(PlatformError::MalformedResponse { .. })
        ));
    }
}
