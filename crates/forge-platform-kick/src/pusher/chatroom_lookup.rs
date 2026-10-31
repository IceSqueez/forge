use std::sync::{Mutex, PoisonError};

use forge_net::{Gateway, GatewayFailure, GatewayRequest};
use serde::Deserialize;
use tokio::time::Instant;
use tracing::debug;

use crate::gateway::CHANNEL_INFO_REFRESH_INTERVAL;
use crate::pusher::error::KickError;

const CHANNELS_PATH: &str = "/channels";
const CHANNEL_LABEL: &str = "/channels/{slug}";
const ERROR_BODY_LIMIT: usize = 200;
const USER_AGENT_HEADER: &str = "User-Agent";
const USER_AGENT: &str = concat!(
    "forge/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/IceSqueez/forge)"
);

#[derive(Debug, Clone)]
pub struct KickChannelInfo {
    pub chatroom_id: u64,
}

struct CachedChannelInfo {
    slug: String,
    info: KickChannelInfo,
    fetched_at: Instant,
}

pub struct ChannelInfoFetcher {
    gateway: Gateway,
    cached: Mutex<Option<CachedChannelInfo>>,
}

impl ChannelInfoFetcher {
    pub fn new(gateway: Gateway) -> Self {
        Self {
            gateway,
            cached: Mutex::new(None),
        }
    }

    pub async fn fetch(&self, slug: &str) -> Result<KickChannelInfo, KickError> {
        if let Some(info) = self.fresh(slug) {
            return Ok(info);
        }
        let info = self.request(slug).await?;
        *self.cached.lock().unwrap_or_else(PoisonError::into_inner) = Some(CachedChannelInfo {
            slug: slug.to_owned(),
            info: info.clone(),
            fetched_at: Instant::now(),
        });
        Ok(info)
    }

    fn fresh(&self, slug: &str) -> Option<KickChannelInfo> {
        let cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        cached
            .as_ref()
            .filter(|entry| {
                entry.slug == slug && entry.fetched_at.elapsed() < CHANNEL_INFO_REFRESH_INTERVAL
            })
            .map(|entry| entry.info.clone())
    }

    async fn request(&self, slug: &str) -> Result<KickChannelInfo, KickError> {
        let request = GatewayRequest::get(format!("{CHANNELS_PATH}/{slug}"))
            .header(USER_AGENT_HEADER, USER_AGENT)
            .label(CHANNEL_LABEL);
        let response = self
            .gateway
            .send(request)
            .await
            .map_err(|failure| channel_info_error(slug, failure))?;

        let body: ChannelResponse =
            response
                .json()
                .map_err(|e| KickError::ChannelInfoUnavailable {
                    slug: slug.to_owned(),
                    reason: format!("failed to parse channel response: {e}"),
                })?;

        let chatroom_id = body.chatroom.as_ref().and_then(|c| c.id).ok_or_else(|| {
            KickError::ChatroomIdNotFound {
                slug: slug.to_owned(),
            }
        })?;

        debug!(chatroom_id, "channel info fetched");

        Ok(KickChannelInfo { chatroom_id })
    }
}

fn channel_info_error(slug: &str, failure: GatewayFailure) -> KickError {
    match (failure.status(), failure.excerpt()) {
        (Some(status), Some(excerpt)) => KickError::Http {
            status,
            body: bounded_body(excerpt.as_str()),
        },
        _ => KickError::ChannelInfoUnavailable {
            slug: slug.to_owned(),
            reason: failure.to_string(),
        },
    }
}

fn bounded_body(body: &str) -> String {
    match body.char_indices().nth(ERROR_BODY_LIMIT) {
        Some((end, _)) => body[..end].to_owned(),
        None => body.to_owned(),
    }
}

#[derive(Deserialize)]
struct ChannelResponse {
    chatroom: Option<ChatroomField>,
}

#[derive(Deserialize)]
struct ChatroomField {
    id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_net::{RateLimiter, TokenBucketRateLimiter};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::endpoints_test_support::{channel_info_at, gateway_waiting_at_most};
    use crate::gateway::{KICK_CHANNEL_INFO_BUDGET_CAPACITY, KICK_CHANNEL_INFO_BUDGET_WINDOW};

    const SLUG: &str = "streamer_slug";
    const SLUG_PATH: &str = "/channels/streamer_slug";
    const SHORT_THROTTLE_WAIT: Duration = Duration::from_millis(50);

    fn live_channel() -> ResponseTemplate {
        ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
            "chatroom": { "id": 12345 },
            "livestream": { "viewer_count": 500, "session_title": "Playing games" }
        }))
    }

    async fn server_replying(template: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(SLUG_PATH))
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    #[tokio::test]
    async fn fetch_maps_chatroom_id_from_the_channel_body() {
        let server = server_replying(live_channel()).await;

        let info = channel_info_at(&server.uri()).fetch(SLUG).await.unwrap();

        assert_eq!(info.chatroom_id, 12345);
    }

    #[tokio::test]
    async fn fetch_identifies_itself_with_the_project_user_agent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(SLUG_PATH))
            .and(header("user-agent", USER_AGENT))
            .respond_with(live_channel())
            .expect(1)
            .mount(&server)
            .await;

        let result = channel_info_at(&server.uri()).fetch(SLUG).await;

        assert!(result.is_ok(), "{result:?}");
    }

    #[tokio::test]
    async fn fetch_reports_an_error_status_as_http_with_its_code() {
        let server = server_replying(
            ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND).set_body_string("not found"),
        )
        .await;

        let err = channel_info_at(&server.uri())
            .fetch(SLUG)
            .await
            .unwrap_err();

        assert!(
            matches!(err, KickError::Http { status: 404, .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_reports_a_missing_chatroom_as_chatroom_id_not_found() {
        let server = server_replying(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                "chatroom": null,
                "livestream": null
            })),
        )
        .await;

        let err = channel_info_at(&server.uri())
            .fetch(SLUG)
            .await
            .unwrap_err();

        assert!(
            matches!(err, KickError::ChatroomIdNotFound { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_reports_an_undecodable_success_body_as_channel_info_unavailable() {
        let server = server_replying(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("<html>"),
        )
        .await;

        let err = channel_info_at(&server.uri())
            .fetch(SLUG)
            .await
            .unwrap_err();

        assert!(
            matches!(&err, KickError::ChannelInfoUnavailable { slug, .. } if slug == SLUG),
            "{err:?}"
        );
    }

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
            let bounded = bounded_body(&body);
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
        let server = server_replying(
            ResponseTemplate::new(reqwest::StatusCode::FORBIDDEN)
                .set_body_string(format!("<html>{}</html>", "x".repeat(5_000))),
        )
        .await;

        let err = channel_info_at(&server.uri())
            .fetch(SLUG)
            .await
            .unwrap_err();

        let KickError::Http { status, body } = err else {
            panic!("expected an Http error, got {err:?}");
        };
        assert_eq!((status, body.chars().count()), (403, ERROR_BODY_LIMIT));
    }

    #[tokio::test]
    async fn transport_failure_is_unavailable_without_the_request_url() {
        let err = channel_info_at("http://127.0.0.1:0")
            .fetch(SLUG)
            .await
            .unwrap_err();

        let KickError::ChannelInfoUnavailable { reason, .. } = err else {
            panic!("expected ChannelInfoUnavailable, got {err:?}");
        };
        assert!(!reason.contains("127.0.0.1"), "url leaked into {reason:?}");
    }

    #[tokio::test]
    async fn a_success_is_served_from_cache_until_the_refresh_interval_elapses() {
        let server = server_replying(live_channel()).await;
        let fetcher = channel_info_at(&server.uri());
        fetcher.fetch(SLUG).await.unwrap();
        let cached = fetcher.fetch(SLUG).await.unwrap();
        let after_cached = request_count(&server).await;
        age_the_cached_entry_by(&fetcher, CHANNEL_INFO_REFRESH_INTERVAL);

        fetcher.fetch(SLUG).await.unwrap();

        assert_eq!(
            (
                cached.chatroom_id,
                after_cached,
                request_count(&server).await
            ),
            (12345, 1, 2)
        );
    }

    fn age_the_cached_entry_by(fetcher: &ChannelInfoFetcher, age: Duration) {
        let mut cached = fetcher.cached.lock().unwrap();
        let entry = cached.as_mut().unwrap();
        entry.fetched_at = entry.fetched_at.checked_sub(age).unwrap();
    }

    #[tokio::test]
    async fn a_cached_success_does_not_answer_for_another_slug() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(live_channel())
            .mount(&server)
            .await;
        let fetcher = channel_info_at(&server.uri());

        fetcher.fetch(SLUG).await.unwrap();
        fetcher.fetch("other_slug").await.unwrap();

        let paths: Vec<String> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect();
        assert_eq!(paths, vec![SLUG_PATH, "/channels/other_slug"]);
    }

    #[tokio::test]
    async fn a_failure_is_not_cached_so_the_next_fetch_asks_again() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(SLUG_PATH))
            .respond_with(ResponseTemplate::new(
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(SLUG_PATH))
            .respond_with(live_channel())
            .mount(&server)
            .await;
        let fetcher = channel_info_at(&server.uri());

        let first = fetcher.fetch(SLUG).await;
        let second = fetcher.fetch(SLUG).await;

        assert!(
            matches!(first, Err(KickError::Http { status: 500, .. })),
            "{first:?}"
        );
        assert_eq!(second.unwrap().chatroom_id, 12345);
    }

    #[tokio::test]
    async fn a_retry_inside_the_budget_window_waits_then_reports_unavailable_without_a_request() {
        let server = server_replying(ResponseTemplate::new(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        ))
        .await;
        let limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            KICK_CHANNEL_INFO_BUDGET_CAPACITY,
            KICK_CHANNEL_INFO_BUDGET_WINDOW,
        ));
        let fetcher = ChannelInfoFetcher::new(gateway_waiting_at_most(
            &server.uri(),
            limiter,
            SHORT_THROTTLE_WAIT,
        ));
        fetcher.fetch(SLUG).await.unwrap_err();
        let started = Instant::now();

        let retry = fetcher.fetch(SLUG).await;

        assert!(
            matches!(&retry, Err(KickError::ChannelInfoUnavailable { slug, .. }) if slug == SLUG),
            "{retry:?}"
        );
        let waited = started.elapsed();
        assert!(waited >= SHORT_THROTTLE_WAIT, "waited {waited:?}");
        assert_eq!(request_count(&server).await, 1);
    }
}
