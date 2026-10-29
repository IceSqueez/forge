use std::sync::Arc;

use forge_net::{GatewayRequest, Secret};
use forge_platform_core::PlatformError;
use futures::future::BoxFuture;

use crate::data_api::{YoutubeDataApi, malformed};

const LOOKUP_COST: u32 = 1;
const CHANNEL_ID_LEN: usize = 24;
const CHANNELS_PATH: &str = "/channels";
const MISSING_SCOPE: &str = "channel lookup scope missing";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeChannelProfile {
    pub channel_id: String,
    pub title: String,
    pub subscriber_count: i64,
    pub view_count: i64,
}

pub struct YoutubeChannelLookup {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
}

impl YoutubeChannelLookup {
    pub fn new(api: YoutubeDataApi, access_token_source: TokenSource) -> Self {
        Self {
            api,
            access_token_source,
        }
    }

    pub async fn lookup(&self, identifier: &str) -> Result<YoutubeChannelProfile, PlatformError> {
        let is_channel_id = identifier.starts_with("UC") && identifier.len() == CHANNEL_ID_LEN;
        let filter_key = if is_channel_id { "id" } else { "forHandle" };

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::get(CHANNELS_PATH)
            .label(CHANNELS_PATH)
            .bearer(Secret::new(token))
            .query("part", "snippet,statistics")
            .query(filter_key, identifier);
        let body: serde_json::Value = self
            .api
            .send(LOOKUP_COST, request)
            .await
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))?
            .json()
            .map_err(malformed)?;

        let item = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| PlatformError::Http {
                status: forge_net::status::NOT_FOUND,
                body: format!("channel not found: {identifier}"),
            })?;

        let channel_id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        let title = item
            .pointer("/snippet/title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let subscriber_count = item
            .pointer("/statistics/subscriberCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let view_count = item
            .pointer("/statistics/viewCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        Ok(YoutubeChannelProfile {
            channel_id: channel_id.to_owned(),
            title: title.to_owned(),
            subscriber_count,
            view_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::endpoints_test_support::data_api_at;

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok("yt-lookup-token".to_owned()) }))
    }

    fn lookup_on(server: &MockServer) -> YoutubeChannelLookup {
        YoutubeChannelLookup::new(data_api_at(&server.uri()), token_source())
    }

    fn one_channel() -> serde_json::Value {
        json!({
            "items": [{
                "id": "UCxyzchannelid",
                "snippet": { "title": "Creator Name" },
                "statistics": { "subscriberCount": "500", "viewCount": "12000" }
            }]
        })
    }

    #[tokio::test]
    async fn lookup_selects_id_filter_for_uc_prefixed_24char_else_handle() {
        let id24 = format!("UC{}", "a".repeat(22));
        assert_eq!(id24.len(), CHANNEL_ID_LEN);

        let cases: Vec<(String, &str)> = vec![
            (id24.clone(), "id"),
            ("@creator".to_owned(), "forHandle"),
            ("UCshort".to_owned(), "forHandle"),
        ];

        for (identifier, expected_key) in cases {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/channels"))
                .respond_with(ResponseTemplate::new(200).set_body_json(one_channel()))
                .mount(&server)
                .await;

            let lookup = lookup_on(&server);
            lookup.lookup(&identifier).await.unwrap();

            let req = &server.received_requests().await.unwrap()[0];
            let pairs: std::collections::HashMap<String, String> =
                req.url.query_pairs().into_owned().collect();
            assert_eq!(
                pairs.get(expected_key).map(String::as_str),
                Some(identifier.as_str()),
                "identifier {identifier} must query by {expected_key}"
            );
        }
    }

    #[tokio::test]
    async fn lookup_parses_channel_into_typed_profile() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(one_channel()))
            .mount(&server)
            .await;

        let profile = lookup_on(&server).lookup("@creator").await.unwrap();
        assert_eq!(
            profile,
            YoutubeChannelProfile {
                channel_id: "UCxyzchannelid".to_owned(),
                title: "Creator Name".to_owned(),
                subscriber_count: 500,
                view_count: 12000,
            }
        );
    }

    #[tokio::test]
    async fn lookup_defaults_hidden_or_unparsable_statistics_to_zero() {
        for (statistics, expected_views, what) in [
            (
                json!({ "hiddenSubscriberCount": true, "viewCount": "9" }),
                9,
                "hidden subscribers",
            ),
            (
                json!({ "subscriberCount": "many", "viewCount": 9 }),
                0,
                "garbage and number",
            ),
            (json!({}), 0, "statistics absent"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/channels"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "items": [{ "id": "UCx", "snippet": { "title": "T" }, "statistics": statistics }]
                })))
                .mount(&server)
                .await;

            let profile = lookup_on(&server).lookup("@creator").await.unwrap();
            assert_eq!(profile.subscriber_count, 0, "{what}");
            assert_eq!(profile.view_count, expected_views, "{what}");
        }
    }

    #[tokio::test]
    async fn lookup_returns_http_404_when_no_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
            .mount(&server)
            .await;

        let err = lookup_on(&server).lookup("@ghost").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 404, .. }),
            "empty items must map to Http 404, got: {err}"
        );
    }

    #[tokio::test]
    async fn lookup_maps_an_undecodable_success_body_to_malformed_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK.as_u16()).set_body_string("not json"),
            )
            .mount(&server)
            .await;

        let err = lookup_on(&server).lookup("@creator").await.unwrap_err();

        assert!(
            matches!(err, PlatformError::MalformedResponse { .. }),
            "{err}"
        );
    }
}
