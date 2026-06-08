use reqwest::header;
use serde::Deserialize;
use tracing::debug;

use crate::error::KickError;

const CHANNEL_API_BASE: &str = "https://kick.com/api/v2/channels";
const USER_AGENT: &str = concat!(
    "forge/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/IceSqueez/forge)"
);

#[derive(Debug, Clone)]
pub struct KickChannelInfo {
    pub chatroom_id: u64,
    pub viewer_count: u64,
    pub stream_title: String,
    pub is_live: bool,
}

pub struct ChannelInfoFetcher {
    slug: String,
    http: reqwest::Client,
    endpoint_base: String,
}

impl ChannelInfoFetcher {
    pub fn new(slug: String, http: reqwest::Client) -> Self {
        Self {
            slug,
            http,
            endpoint_base: CHANNEL_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_endpoint(
        slug: String,
        http: reqwest::Client,
        endpoint_base: String,
    ) -> Self {
        Self {
            slug,
            http,
            endpoint_base,
        }
    }

    /// Fetches channel info from the unofficial v2 endpoint.
    ///
    /// Returns `KickError::ChannelInfoUnavailable` if the endpoint is unreachable or returns a
    /// non-2xx status. Callers must retry with backoff on this variant.
    pub async fn fetch(&self) -> Result<KickChannelInfo, KickError> {
        let url = format!("{}/{}", self.endpoint_base, self.slug);
        let response = self
            .http
            .get(&url)
            .header(header::USER_AGENT, USER_AGENT)
            .send()
            .await
            .map_err(|e| KickError::ChannelInfoUnavailable {
                slug: self.slug.clone(),
                reason: e.to_string(),
            })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(KickError::Http { status, body });
        }

        let body: ChannelResponse =
            response
                .json()
                .await
                .map_err(|e| KickError::ChannelInfoUnavailable {
                    slug: self.slug.clone(),
                    reason: format!("failed to parse channel response: {e}"),
                })?;

        let chatroom_id = body.chatroom.as_ref().and_then(|c| c.id).ok_or_else(|| {
            KickError::ChatroomIdNotFound {
                slug: self.slug.clone(),
            }
        })?;

        let viewer_count = body.livestream.as_ref().map_or(0, |l| l.viewer_count);
        let stream_title = body
            .livestream
            .as_ref()
            .and_then(|l| l.session_title.clone())
            .unwrap_or_default();
        let is_live = body.livestream.is_some();

        debug!(
            slug = %self.slug,
            chatroom_id,
            viewer_count,
            is_live,
            "channel info fetched"
        );

        Ok(KickChannelInfo {
            chatroom_id,
            viewer_count,
            stream_title,
            is_live,
        })
    }
}

#[derive(Deserialize)]
struct ChannelResponse {
    chatroom: Option<ChatroomField>,
    livestream: Option<LivestreamField>,
}

#[derive(Deserialize)]
struct ChatroomField {
    id: Option<u64>,
}

#[derive(Deserialize)]
struct LivestreamField {
    viewer_count: u64,
    session_title: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_returns_channel_info_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/streamer_slug"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "chatroom": { "id": 12345 },
                "livestream": {
                    "viewer_count": 500,
                    "session_title": "Playing games"
                }
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let fetcher =
            ChannelInfoFetcher::with_endpoint("streamer_slug".to_owned(), http, server.uri());
        let info = fetcher.fetch().await.unwrap();
        assert_eq!(info.chatroom_id, 12345);
        assert_eq!(info.viewer_count, 500);
        assert_eq!(info.stream_title, "Playing games");
        assert!(info.is_live);
    }

    #[tokio::test]
    async fn fetch_returns_error_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/unknown_slug"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let fetcher =
            ChannelInfoFetcher::with_endpoint("unknown_slug".to_owned(), http, server.uri());
        let err = fetcher.fetch().await.unwrap_err();
        assert!(matches!(err, KickError::Http { status: 404, .. }));
    }

    #[tokio::test]
    async fn fetch_returns_error_when_chatroom_id_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/no_chatroom"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "chatroom": null,
                "livestream": null
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let fetcher =
            ChannelInfoFetcher::with_endpoint("no_chatroom".to_owned(), http, server.uri());
        let err = fetcher.fetch().await.unwrap_err();
        assert!(matches!(err, KickError::ChatroomIdNotFound { .. }));
    }

    #[tokio::test]
    async fn fetch_sets_is_live_false_when_no_livestream() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/offline_slug"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "chatroom": { "id": 99 },
                "livestream": null
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let fetcher =
            ChannelInfoFetcher::with_endpoint("offline_slug".to_owned(), http, server.uri());
        let info = fetcher.fetch().await.unwrap();
        assert!(!info.is_live);
        assert_eq!(info.viewer_count, 0);
    }
}
