use reqwest::header;
use serde::Deserialize;
use tracing::debug;

use crate::error::KickError;

const CHANNEL_API_BASE: &str = "https://kick.com/api/v2/channels";
const ERROR_BODY_LIMIT: usize = 200;
const USER_AGENT: &str = concat!(
    "forge/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/IceSqueez/forge)"
);

#[derive(Debug, Clone)]
pub struct KickChannelInfo {
    pub chatroom_id: u64,
    pub viewer_count: u64,
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

    /// Callers must retry with backoff on `KickError::ChannelInfoUnavailable`.
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
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let body = response.text().await.unwrap_or_default();
            return Err(KickError::Http {
                status,
                body: bounded_body(body),
            });
        }

        let body: ChannelResponse =
            response
                .json()
                .await
                .map_err(|e| KickError::ChannelInfoUnavailable {
                    slug: self.slug.clone(),
                    reason: format!("failed to parse channel response: {}", e.without_url()),
                })?;

        let chatroom_id = body.chatroom.as_ref().and_then(|c| c.id).ok_or_else(|| {
            KickError::ChatroomIdNotFound {
                slug: self.slug.clone(),
            }
        })?;

        let viewer_count = body.livestream.as_ref().map_or(0, |l| l.viewer_count);
        let is_live = body.livestream.is_some();

        debug!(chatroom_id, viewer_count, is_live, "channel info fetched");

        Ok(KickChannelInfo {
            chatroom_id,
            viewer_count,
            is_live,
        })
    }
}

// Why: this endpoint sits behind a challenge layer that answers errors with a full HTML page,
// and the body reaches sub-action error text and run history.
fn bounded_body(body: String) -> String {
    match body.char_indices().nth(ERROR_BODY_LIMIT) {
        Some((end, _)) => body[..end].to_owned(),
        None => body,
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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

    /// The challenge layer answers with an HTML page, and the body travels into sub-action error
    /// text and run history - so the cut must land on a char boundary rather than a byte offset.
    #[test]
    fn bounded_body_cuts_at_the_char_limit_without_splitting_a_char() {
        let cases = [
            "short".to_owned(),
            "a".repeat(ERROR_BODY_LIMIT),
            "a".repeat(ERROR_BODY_LIMIT + 1),
            "\u{1f600}".repeat(ERROR_BODY_LIMIT * 2),
            "\u{456}".repeat(ERROR_BODY_LIMIT - 1),
        ];

        for body in cases {
            let expected_chars = body.chars().count().min(ERROR_BODY_LIMIT);
            let bounded = bounded_body(body.clone());
            assert_eq!(
                bounded.chars().count(),
                expected_chars,
                "wrong length for a {}-char body",
                body.chars().count()
            );
            assert!(body.starts_with(&bounded), "cut changed the body prefix");
        }
    }

    #[tokio::test]
    async fn fetch_bounds_a_challenge_page_body_on_an_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/blocked_slug"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string(format!("<html>{}</html>", "x".repeat(5_000))),
            )
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let fetcher =
            ChannelInfoFetcher::with_endpoint("blocked_slug".to_owned(), http, server.uri());
        let err = fetcher.fetch().await.unwrap_err();
        let KickError::Http { status, body } = err else {
            panic!("expected an Http error, got {err:?}");
        };
        assert_eq!(status, 403);
        assert_eq!(body.chars().count(), ERROR_BODY_LIMIT);
    }

    /// Invariant #7: the request URL carries no secret here, but the same rendering is reused for
    /// token-bearing calls - `without_url` is the guarantee under test.
    #[tokio::test]
    async fn transport_failure_reason_omits_the_request_url() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let endpoint = format!("http://127.0.0.1:{port}");

        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();
        let fetcher = ChannelInfoFetcher::with_endpoint("some_slug".to_owned(), http, endpoint);
        let err = fetcher.fetch().await.unwrap_err();
        let KickError::ChannelInfoUnavailable { reason, .. } = err else {
            panic!("expected ChannelInfoUnavailable, got {err:?}");
        };
        assert!(!reason.contains("127.0.0.1"), "url leaked into {reason:?}");
        assert!(
            !reason.contains(&port.to_string()),
            "url leaked into {reason:?}"
        );
    }
}
