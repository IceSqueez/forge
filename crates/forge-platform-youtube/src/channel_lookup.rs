use std::collections::BTreeMap;
use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_types::Variant;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const LOOKUP_COST: u32 = 1;
const CHANNEL_ID_LEN: usize = 24;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeChannelLookup {
    client: reqwest::Client,
    access_token_source: TokenSource,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeChannelLookup {
    pub fn new(access_token_source: TokenSource, quota: Arc<Mutex<QuotaState>>) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
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

    /// `UC`-prefixed 24-char identifiers query by channel id; anything else queries by handle.
    pub async fn lookup(&self, identifier: &str) -> Result<Variant, PlatformError> {
        let is_channel_id = identifier.starts_with("UC") && identifier.len() == CHANNEL_ID_LEN;
        let filter_key = if is_channel_id { "id" } else { "forHandle" };

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(LOOKUP_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/channels", self.api_base);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet,statistics"), (filter_key, identifier)])
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

        let item = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| PlatformError::Http {
                status: 404,
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

        Ok(Variant::Object(BTreeMap::from([
            (
                "channel_id".to_owned(),
                Variant::String(channel_id.to_owned()),
            ),
            ("title".to_owned(), Variant::String(title.to_owned())),
            (
                "subscriber_count".to_owned(),
                Variant::Int(subscriber_count),
            ),
            ("view_count".to_owned(), Variant::Int(view_count)),
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
                    reason: "channel lookup scope missing".to_owned(),
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-lookup-secret-token";

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn lookup_on(server: &MockServer) -> YoutubeChannelLookup {
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        YoutubeChannelLookup::new(token_source(), quota).with_api_base(server.uri())
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
    async fn lookup_parses_channel_into_variant_object() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(one_channel()))
            .mount(&server)
            .await;

        let result = lookup_on(&server).lookup("@creator").await.unwrap();
        let Variant::Object(map) = result else {
            panic!("expected Object, got {result:?}");
        };
        assert_eq!(
            map.get("channel_id"),
            Some(&Variant::String("UCxyzchannelid".to_owned()))
        );
        assert_eq!(
            map.get("title"),
            Some(&Variant::String("Creator Name".to_owned()))
        );
        assert_eq!(map.get("subscriber_count"), Some(&Variant::Int(500)));
        assert_eq!(map.get("view_count"), Some(&Variant::Int(12000)));
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
    async fn lookup_maps_403_operation_not_supported_to_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(403).set_body_string(
                    r#"{"error":{"errors":[{"reason":"operationNotSupported"}]}}"#,
                ),
            )
            .mount(&server)
            .await;

        let err = lookup_on(&server).lookup("@creator").await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { .. }),
            "403 operationNotSupported must map to Auth, got: {err}"
        );
    }

    #[tokio::test]
    async fn lookup_error_does_not_leak_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let err = lookup_on(&server).lookup("@creator").await.unwrap_err();
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
