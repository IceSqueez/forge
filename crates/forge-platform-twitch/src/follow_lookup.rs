use async_trait::async_trait;
use forge_platform_core::{FollowLookup, FollowStatus};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::builtin::TwitchIntegrationBundle;
use crate::custom_rewards::first_row;
use crate::helix::{HelixMethod, HelixRequest};

const CHANNEL_FOLLOWERS_PATH: &str = "/helix/channels/followers";
const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const USER_ID_PARAM: &str = "user_id";
const FIRST_PARAM: &str = "first";
const SINGLE_RESULT: u8 = 1;
const FOLLOWED_AT_KEY: &str = "followed_at";

fn channel_follower_request(broadcaster_id: &str, viewer_id: &str) -> HelixRequest {
    HelixRequest::new(HelixMethod::Get, CHANNEL_FOLLOWERS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(USER_ID_PARAM, viewer_id)
        .query(FIRST_PARAM, SINGLE_RESULT.to_string())
}

fn status_from_response(response: &serde_json::Value) -> FollowStatus {
    let Some(row) = first_row(response) else {
        return FollowStatus::NotFollowing;
    };
    row[FOLLOWED_AT_KEY]
        .as_str()
        .and_then(|raw| OffsetDateTime::parse(raw, &Rfc3339).ok())
        .map_or(FollowStatus::Unavailable, FollowStatus::FollowedSince)
}

#[async_trait]
impl FollowLookup for TwitchIntegrationBundle {
    async fn follow_status(&self, viewer_id: &str) -> FollowStatus {
        let broadcaster_id = self.broadcaster_id();
        if broadcaster_id.is_empty() || viewer_id.is_empty() {
            return FollowStatus::Unavailable;
        }
        if viewer_id == broadcaster_id {
            return FollowStatus::NotFollowing;
        }
        match self
            .helix()
            .execute(channel_follower_request(broadcaster_id, viewer_id))
            .await
        {
            Ok(response) => status_from_response(&response),
            Err(_) => FollowStatus::Unavailable,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use forge_events::EventPublisher;
    use forge_platform_core::{
        EndpointSurface, PlatformEndpoints, PlatformError, RateLimitOutcome, RateLimiter,
    };
    use forge_types::OAuthToken;
    use serde_json::{Value, json};
    use tokio::sync::watch;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::BroadcasterTier;
    use crate::chat::ChatConnectionState;
    use crate::event_channel::PlatformEventChannel;
    use crate::helix::{
        HelixError, HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport,
    };
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, endpoints_with, unreachable_twitch_endpoints,
    };
    use crate::subscriptions::SubscriptionTracker;

    const BROADCASTER: &str = "100000001";
    const VIEWER: &str = "200000042";
    const FOLLOWED_AT: &str = "2023-04-05T06:07:08Z";
    const FOLLOWED_AT_UNIX: i64 = 1_680_674_828;

    struct StaleToken;

    #[async_trait]
    impl HelixTokenSource for StaleToken {
        async fn access_token(&self) -> Result<OAuthToken, HelixError> {
            Ok(OAuthToken::new("stale-token"))
        }
    }

    #[derive(Default)]
    struct CountingRefresher {
        refreshes: AtomicUsize,
    }

    #[async_trait]
    impl HelixTokenRefresher for CountingRefresher {
        async fn refresh(&self, _failed_token: &OAuthToken) -> Result<OAuthToken, HelixError> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            Ok(OAuthToken::new("fresh-token"))
        }
    }

    struct GrantLimiter;

    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }

        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn followed_at() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(FOLLOWED_AT_UNIX).unwrap()
    }

    fn follower_page(followed_at: Value) -> Value {
        json!({
            "total": 1,
            "data": [{
                "user_id": VIEWER,
                "user_login": "alice",
                "user_name": "alice",
                "followed_at": followed_at,
            }],
            "pagination": {},
        })
    }

    fn bundle(
        transport: Arc<dyn HelixTransport>,
        broadcaster_id: &str,
    ) -> Arc<TwitchIntegrationBundle> {
        let (_tx, rx) = watch::channel(ChatConnectionState::Connected);
        TwitchIntegrationBundle::for_test_with_transport(
            Some("streamer".to_owned()),
            rx,
            SubscriptionTracker::default(),
            Arc::new(MockCreds::with_identity()),
            BroadcasterTier::Affiliate,
            transport,
            broadcaster_id,
        )
    }

    fn answering(body: Value) -> (Arc<MockTransport>, Arc<TwitchIntegrationBundle>) {
        let transport = Arc::new(MockTransport::returning(Ok(body)));
        let bundle = bundle(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            BROADCASTER,
        );
        (transport, bundle)
    }

    fn over_http(
        endpoints: &PlatformEndpoints,
        refresher: Arc<CountingRefresher>,
    ) -> Arc<TwitchIntegrationBundle> {
        let publisher: Arc<dyn EventPublisher> = Arc::new(PlatformEventChannel::new());
        let transport = HelixHttpTransport::new(
            endpoints,
            Arc::new(GrantLimiter),
            publisher,
            "test-client".to_owned(),
            Arc::new(StaleToken),
        )
        .with_refresher(refresher);
        bundle(Arc::new(transport), BROADCASTER)
    }

    fn api_override(server: &MockServer) -> PlatformEndpoints {
        endpoints_with(&[(EndpointSurface::TwitchApi, &server.uri())])
    }

    #[tokio::test]
    async fn follow_status_asks_the_api_override_for_one_row_of_this_viewer() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/helix/channels/followers"))
            .and(query_param("broadcaster_id", BROADCASTER))
            .and(query_param("user_id", VIEWER))
            .and(query_param("first", "1"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK.as_u16())
                    .set_body_json(follower_page(json!(FOLLOWED_AT))),
            )
            .mount(&server)
            .await;
        let bundle = over_http(&api_override(&server), Arc::default());

        let status = bundle.follow_status(VIEWER).await;

        assert_eq!(status, FollowStatus::FollowedSince(followed_at()));
    }

    #[tokio::test]
    async fn follow_status_maps_each_follower_page_shape() {
        let half_second_later = followed_at() + time::Duration::milliseconds(500);
        let cases = [
            (
                "utc instant",
                follower_page(json!(FOLLOWED_AT)),
                FollowStatus::FollowedSince(followed_at()),
            ),
            (
                "offset instant",
                follower_page(json!("2023-04-05T09:07:08+03:00")),
                FollowStatus::FollowedSince(followed_at()),
            ),
            (
                "fractional seconds",
                follower_page(json!("2023-04-05T06:07:08.5Z")),
                FollowStatus::FollowedSince(half_second_later),
            ),
            (
                "empty data",
                json!({ "total": 7, "data": [], "pagination": {} }),
                FollowStatus::NotFollowing,
            ),
            (
                "missing followed_at",
                json!({ "total": 1, "data": [{ "user_id": VIEWER }], "pagination": {} }),
                FollowStatus::Unavailable,
            ),
            (
                "null followed_at",
                follower_page(Value::Null),
                FollowStatus::Unavailable,
            ),
            (
                "numeric followed_at",
                follower_page(json!(FOLLOWED_AT_UNIX)),
                FollowStatus::Unavailable,
            ),
            (
                "empty followed_at",
                follower_page(json!("")),
                FollowStatus::Unavailable,
            ),
            (
                "malformed followed_at",
                follower_page(json!("yesterday")),
                FollowStatus::Unavailable,
            ),
            (
                "date without time",
                follower_page(json!("2023-04-05")),
                FollowStatus::Unavailable,
            ),
        ];

        for (label, body, expected) in cases {
            let (_transport, bundle) = answering(body);

            assert_eq!(bundle.follow_status(VIEWER).await, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn follow_status_is_unavailable_when_the_api_refuses_or_fails() {
        let cases = [
            (
                "403",
                ResponseTemplate::new(reqwest::StatusCode::FORBIDDEN.as_u16()),
            ),
            (
                "429",
                ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16())
                    .insert_header("Retry-After", "1"),
            ),
            (
                "500",
                ResponseTemplate::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
            ),
            (
                "503",
                ResponseTemplate::new(reqwest::StatusCode::SERVICE_UNAVAILABLE.as_u16()),
            ),
            (
                "200 with a non-json body",
                ResponseTemplate::new(reqwest::StatusCode::OK.as_u16()).set_body_string("<html>"),
            ),
        ];

        for (label, response) in cases {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/helix/channels/followers"))
                .respond_with(response)
                .mount(&server)
                .await;
            let bundle = over_http(&api_override(&server), Arc::default());

            assert_eq!(
                bundle.follow_status(VIEWER).await,
                FollowStatus::Unavailable,
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn follow_status_is_unavailable_when_the_refreshed_token_is_rejected_too() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/helix/channels/followers"))
            .respond_with(ResponseTemplate::new(
                reqwest::StatusCode::UNAUTHORIZED.as_u16(),
            ))
            .mount(&server)
            .await;
        let refresher = Arc::new(CountingRefresher::default());
        let bundle = over_http(&api_override(&server), Arc::clone(&refresher));

        let status = bundle.follow_status(VIEWER).await;

        assert_eq!(
            (
                status,
                refresher.refreshes.load(Ordering::SeqCst),
                server.received_requests().await.unwrap().len(),
            ),
            (FollowStatus::Unavailable, 1, 2),
        );
    }

    #[tokio::test]
    async fn follow_status_is_unavailable_when_the_api_host_is_unreachable() {
        let bundle = over_http(&unreachable_twitch_endpoints(), Arc::default());

        assert_eq!(
            bundle.follow_status(VIEWER).await,
            FollowStatus::Unavailable
        );
    }

    #[tokio::test]
    async fn the_broadcaster_looking_up_themself_is_not_following_without_a_request() {
        let (transport, bundle) = answering(follower_page(json!(FOLLOWED_AT)));

        let status = bundle.follow_status(BROADCASTER).await;

        assert_eq!(
            (status, transport.call_count()),
            (FollowStatus::NotFollowing, 0)
        );
    }

    #[tokio::test]
    async fn a_missing_broadcaster_or_viewer_id_is_unavailable_without_a_request() {
        for (broadcaster_id, viewer_id) in [("", VIEWER), (BROADCASTER, ""), ("", "")] {
            let transport = Arc::new(MockTransport::returning(Ok(follower_page(json!(
                FOLLOWED_AT
            )))));
            let bundle = bundle(
                Arc::clone(&transport) as Arc<dyn HelixTransport>,
                broadcaster_id,
            );

            let status = bundle.follow_status(viewer_id).await;

            assert_eq!(
                (status, transport.call_count()),
                (FollowStatus::Unavailable, 0),
                "broadcaster {broadcaster_id:?} / viewer {viewer_id:?}"
            );
        }
    }
}
