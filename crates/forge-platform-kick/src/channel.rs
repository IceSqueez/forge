use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimiter, acquire_or_wait};
use serde::{Deserialize, Serialize};

const CHANNELS_ENDPOINT: &str = "https://api.kick.com/public/v1/channels";

pub struct KickChannel {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    channels_endpoint: String,
}

pub struct ChannelSnapshot {
    pub broadcaster_user_id: u64,
    pub slug: String,
    pub is_live: bool,
    pub stream_title: String,
    pub category_id: u64,
    pub category_name: String,
    pub viewer_count: u64,
    pub started_at: String,
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
    broadcaster_user_id: u64,
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
    id: u64,
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
    start_time: String,
}

impl KickChannel {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            channels_endpoint: CHANNELS_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api_base(mut self, base: String) -> Self {
        self.channels_endpoint = format!("{base}/channels");
        self
    }

    pub async fn update_info(
        &self,
        token: &str,
        title: Option<String>,
        category_id: Option<u64>,
        tags: Option<Vec<String>>,
    ) -> Result<(), PlatformError> {
        self.acquire_slot().await?;

        let body = UpdateBody {
            stream_title: title,
            category_id,
            custom_tags: tags,
        };

        let response = self
            .client
            .patch(&self.channels_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        map_channel_response(response).await
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
        self.acquire_slot().await?;

        let mut request = self
            .client
            .get(&self.channels_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"));
        if let Some(slug) = slug {
            request = request.query(&[("slug", slug)]);
        }

        let response = request.send().await.map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return map_channel_error(status, response).await;
        }

        let envelope: ChannelsEnvelope =
            response.json().await.map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let channel = envelope
            .data
            .into_iter()
            .next()
            .ok_or_else(|| PlatformError::Http {
                status: 0,
                body: "channels GET returned no data".to_owned(),
            })?;

        Ok(ChannelSnapshot {
            broadcaster_user_id: channel.broadcaster_user_id,
            slug: channel.slug,
            is_live: channel.stream.is_live,
            stream_title: channel.stream_title,
            category_id: channel.category.id,
            category_name: channel.category.name,
            viewer_count: channel.stream.viewer_count,
            started_at: channel.stream.start_time,
        })
    }

    async fn acquire_slot(&self) -> Result<(), PlatformError> {
        acquire_or_wait(self.limiter.as_ref(), 1).await
    }
}

async fn map_channel_response(response: reqwest::Response) -> Result<(), PlatformError> {
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(());
    }

    map_channel_error(status, response).await
}

async fn map_channel_error<T>(
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
            reason: "channel token rejected (401)".to_owned(),
        }),
        403 => Err(PlatformError::Auth {
            reason: "channel forbidden (403); check channel scope".to_owned(),
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
    use forge_platform_core::RateLimitOutcome;
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

    fn channel_on(server: &MockServer) -> KickChannel {
        KickChannel::new(Arc::new(GrantLimiter)).with_api_base(server.uri())
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
            .respond_with(ResponseTemplate::new(200))
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
            .respond_with(ResponseTemplate::new(200))
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
    async fn auth_status_maps_to_auth_error() {
        for status in [401_u16, 403] {
            let server = MockServer::start().await;
            Mock::given(method("PATCH"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;

            let err = channel_on(&server)
                .update_info("tok", Some("t".to_owned()), None, None)
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
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(422))
            .mount(&server)
            .await;

        let err = channel_on(&server)
            .update_info("tok", Some("t".to_owned()), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Http { status: 422, .. }));
    }

    #[tokio::test]
    async fn rate_limited_status_maps_to_rate_limited_with_parsed_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "57"))
            .mount(&server)
            .await;

        let err = channel_on(&server)
            .update_info("tok", Some("t".to_owned()), None, None)
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
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = KickChannel::new(Arc::new(ExhaustedLimiter)).with_api_base(server.uri());
        let err = client
            .update_info("tok", Some("t".to_owned()), None, None)
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an exhausted limiter must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn get_channel_maps_first_data_element_onto_snapshot_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
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
        assert_eq!(snapshot.started_at, "2026-07-24T10:00:00Z");
        assert_eq!(snapshot.stream_title, "Speedrun Night");
        assert_eq!(snapshot.category_id, 77);
        assert_eq!(snapshot.category_name, "Just Chatting");
        assert_eq!(snapshot.viewer_count, 1234);
    }

    #[tokio::test]
    async fn get_channel_empty_data_maps_to_http_status_zero_no_data_guard() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": []
            })))
            .mount(&server)
            .await;

        let result = channel_on(&server).get_channel("tok").await;
        assert!(matches!(result, Err(PlatformError::Http { status: 0, .. })));
    }

    #[tokio::test]
    async fn get_channel_missing_optional_objects_default_to_zero_and_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "stream_title": "Offline"
                }]
            })))
            .mount(&server)
            .await;

        let snapshot = channel_on(&server).get_channel("tok").await.unwrap();
        assert_eq!(snapshot.category_id, 0);
        assert_eq!(snapshot.category_name, "");
        // Why: Kick omits "stream" entirely off-air. Defaulting to 0 (not an error) is what
        // lets an offline channel resolve to ViewerReport::Absent instead of a failed poll.
        assert_eq!(snapshot.viewer_count, 0);
    }
}
