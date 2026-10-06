use async_trait::async_trait;
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility,
};
use reqwest::StatusCode;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::builtin::TwitchIntegrationBundle;
use crate::helix::{HelixError, HelixMethod, HelixRequest};

const BANNED_USERS_PATH: &str = "/helix/moderation/banned";
const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const FIRST_PARAM: &str = "first";
const AFTER_PARAM: &str = "after";
const MAX_PAGE_SIZE: u8 = 100;

#[derive(Deserialize)]
struct BannedUsersPage {
    #[serde(default)]
    data: Vec<BannedUserRow>,
    #[serde(default)]
    pagination: Pagination,
}

#[derive(Deserialize, Default)]
struct Pagination {
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct BannedUserRow {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    user_login: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    expires_at: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    moderator_login: String,
    #[serde(default)]
    moderator_name: String,
}

fn banned_users_request(broadcaster_id: &str, after: Option<&BanPageToken>) -> HelixRequest {
    let request = HelixRequest::new(HelixMethod::Get, BANNED_USERS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(FIRST_PARAM, MAX_PAGE_SIZE.to_string());
    match after
        .map(BanPageToken::as_str)
        .filter(|raw| !raw.is_empty())
    {
        Some(cursor) => request.query(AFTER_PARAM, cursor),
        None => request,
    }
}

fn non_empty(raw: String) -> Option<String> {
    (!raw.is_empty()).then_some(raw)
}

fn parse_instant(raw: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

fn duration_of(expires_at: &str) -> Option<BanDuration> {
    if expires_at.is_empty() {
        return Some(BanDuration::Permanent);
    }
    parse_instant(expires_at).map(BanDuration::Until)
}

fn entry_from_row(row: BannedUserRow) -> Option<BanEntry> {
    if row.user_id.is_empty() {
        return None;
    }
    let duration = duration_of(&row.expires_at)?;
    Some(BanEntry {
        name: non_empty(row.user_name)
            .or_else(|| non_empty(row.user_login))
            .unwrap_or_else(|| row.user_id.clone()),
        viewer_id: row.user_id,
        reason: non_empty(row.reason),
        moderator: non_empty(row.moderator_name).or_else(|| non_empty(row.moderator_login)),
        created_at: parse_instant(&row.created_at),
        duration,
        unban: UnbanAbility::Allowed,
    })
}

fn page_from_response(response: serde_json::Value) -> BanListOutcome {
    let Ok(page) = serde_json::from_value::<BannedUsersPage>(response) else {
        return BanListOutcome::Unavailable(BanListUnavailable::Transport);
    };
    BanListOutcome::Page(BanPage {
        entries: page.data.into_iter().filter_map(entry_from_row).collect(),
        next: page
            .pagination
            .cursor
            .and_then(non_empty)
            .map(BanPageToken::new),
    })
}

fn unavailability_of(error: &HelixError) -> BanListUnavailable {
    match error {
        HelixError::Credentials(_) => BanListUnavailable::NotConnected,
        HelixError::ReauthRequired => BanListUnavailable::MissingScope,
        HelixError::Http { status, .. } if *status == StatusCode::FORBIDDEN.as_u16() => {
            BanListUnavailable::MissingScope
        }
        HelixError::Http { .. } | HelixError::RateLimited | HelixError::Transport(_) => {
            BanListUnavailable::Transport
        }
    }
}

#[async_trait]
impl BanListSource for TwitchIntegrationBundle {
    async fn list_bans(&self, after: Option<&BanPageToken>) -> BanListOutcome {
        let broadcaster_id = self.broadcaster_id();
        if broadcaster_id.is_empty() {
            return BanListOutcome::Unavailable(BanListUnavailable::NotConnected);
        }
        match self
            .helix()
            .execute(banned_users_request(broadcaster_id, after))
            .await
        {
            Ok(response) => page_from_response(response),
            Err(error) => BanListOutcome::Unavailable(unavailability_of(&error)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use forge_events::EventPublisher;
    use forge_platform_core::{EndpointSurface, PlatformError, RateLimitOutcome, RateLimiter};
    use forge_types::OAuthToken;
    use serde_json::{Value, json};
    use tokio::sync::watch;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::BroadcasterTier;
    use crate::chat::ChatConnectionState;
    use crate::event_channel::PlatformEventChannel;
    use crate::helix::{HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport};
    use crate::sub_actions::test_support::{MockCreds, MockTransport, endpoints_with};
    use crate::subscriptions::SubscriptionTracker;

    const BROADCASTER: &str = "100000001";
    const VIEWER: &str = "200000042";
    const BANNED_AT: &str = "2026-10-06T10:00:00Z";
    const BANNED_AT_UNIX: i64 = 1_791_280_800;
    const EXPIRES_AT: &str = "2026-10-06T10:10:00Z";

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

    fn instant(unix: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(unix).unwrap()
    }

    fn row() -> Value {
        json!({
            "user_id": VIEWER,
            "user_login": "alice",
            "user_name": "Alice",
            "expires_at": "",
            "created_at": BANNED_AT,
            "reason": "spam",
            "moderator_id": "300000001",
            "moderator_login": "modbot",
            "moderator_name": "ModBot",
        })
    }

    fn row_with(fields: &[(&str, Value)]) -> Value {
        let mut row = row();
        for (field, value) in fields {
            row[*field] = value.clone();
        }
        row
    }

    fn row_without(field: &str) -> Value {
        let mut row = row();
        row.as_object_mut().unwrap().remove(field);
        row
    }

    fn entry() -> BanEntry {
        BanEntry {
            viewer_id: VIEWER.to_owned(),
            name: "Alice".to_owned(),
            reason: Some("spam".to_owned()),
            moderator: Some("ModBot".to_owned()),
            created_at: Some(instant(BANNED_AT_UNIX)),
            duration: BanDuration::Permanent,
            unban: UnbanAbility::Allowed,
        }
    }

    fn page_of(rows: Vec<Value>, pagination: Value) -> Value {
        json!({ "data": rows, "pagination": pagination })
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

    fn answering(
        response: Result<Value, HelixError>,
    ) -> (Arc<MockTransport>, Arc<TwitchIntegrationBundle>) {
        let transport = Arc::new(MockTransport::returning(response));
        let bundle = bundle(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            BROADCASTER,
        );
        (transport, bundle)
    }

    async fn page_for(response: Value) -> BanPage {
        let (_transport, bundle) = answering(Ok(response));
        match bundle.list_bans(None).await {
            BanListOutcome::Page(page) => page,
            other => panic!("expected a page, got {other:?}"),
        }
    }

    fn over_http(
        server: &MockServer,
        refresher: Arc<CountingRefresher>,
    ) -> Arc<TwitchIntegrationBundle> {
        let publisher: Arc<dyn EventPublisher> = Arc::new(PlatformEventChannel::new());
        let endpoints = endpoints_with(&[(EndpointSurface::TwitchApi, &server.uri())]);
        let transport = HelixHttpTransport::new(
            &endpoints,
            Arc::new(GrantLimiter),
            publisher,
            "test-client".to_owned(),
            Arc::new(StaleToken),
        )
        .with_refresher(refresher);
        bundle(Arc::new(transport), BROADCASTER)
    }

    fn query_value(request: &HelixRequest, key: &str) -> Option<String> {
        request
            .query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }

    #[tokio::test]
    async fn list_bans_reads_the_first_page_of_banned_users_from_the_api_override() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/helix/moderation/banned"))
            .and(query_param("broadcaster_id", BROADCASTER))
            .and(query_param("first", "100"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK.as_u16())
                    .set_body_json(page_of(vec![row()], json!({}))),
            )
            .mount(&server)
            .await;
        let bundle = over_http(&server, Arc::default());

        let outcome = bundle.list_bans(None).await;

        assert_eq!(
            outcome,
            BanListOutcome::Page(BanPage {
                entries: vec![entry()],
                next: None,
            })
        );
    }

    #[tokio::test]
    async fn list_bans_maps_each_banned_user_row_shape() {
        let cases = [
            ("permanent ban", row(), Some(entry())),
            (
                "timeout",
                row_with(&[("expires_at", json!(EXPIRES_AT))]),
                Some(BanEntry {
                    duration: BanDuration::Until(instant(BANNED_AT_UNIX + 600)),
                    ..entry()
                }),
            ),
            (
                "timeout with an offset",
                row_with(&[("expires_at", json!("2026-10-06T13:10:00+03:00"))]),
                Some(BanEntry {
                    duration: BanDuration::Until(instant(BANNED_AT_UNIX + 600)),
                    ..entry()
                }),
            ),
            (
                "missing expires_at",
                row_without("expires_at"),
                Some(entry()),
            ),
            (
                "unparsable expires_at",
                row_with(&[("expires_at", json!("tomorrow"))]),
                None,
            ),
            ("empty user_id", row_with(&[("user_id", json!(""))]), None),
            ("missing user_id", row_without("user_id"), None),
            (
                "empty reason",
                row_with(&[("reason", json!(""))]),
                Some(BanEntry {
                    reason: None,
                    ..entry()
                }),
            ),
            (
                "moderator without a display name",
                row_with(&[("moderator_name", json!(""))]),
                Some(BanEntry {
                    moderator: Some("modbot".to_owned()),
                    ..entry()
                }),
            ),
            (
                "no moderator",
                row_with(&[
                    ("moderator_name", json!("")),
                    ("moderator_login", json!("")),
                ]),
                Some(BanEntry {
                    moderator: None,
                    ..entry()
                }),
            ),
            (
                "user without a display name",
                row_with(&[("user_name", json!(""))]),
                Some(BanEntry {
                    name: "alice".to_owned(),
                    ..entry()
                }),
            ),
            (
                "user without a display name or login",
                row_with(&[("user_name", json!("")), ("user_login", json!(""))]),
                Some(BanEntry {
                    name: VIEWER.to_owned(),
                    ..entry()
                }),
            ),
            (
                "unparsable created_at",
                row_with(&[("created_at", json!("long ago"))]),
                Some(BanEntry {
                    created_at: None,
                    ..entry()
                }),
            ),
            (
                "unicode display name",
                row_with(&[("user_name", json!("Алиса"))]),
                Some(BanEntry {
                    name: "Алиса".to_owned(),
                    ..entry()
                }),
            ),
        ];

        for (label, row, expected) in cases {
            let page = page_for(page_of(vec![row], json!({}))).await;

            assert_eq!(
                page.entries,
                expected.into_iter().collect::<Vec<_>>(),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn list_bans_drops_only_the_unusable_row_and_keeps_its_neighbours_in_order() {
        let first = row_with(&[("user_id", json!("200000001"))]);
        let second = row_with(&[("user_id", json!("200000003"))]);

        let page = page_for(page_of(
            vec![first, row_with(&[("expires_at", json!("soon"))]), second],
            json!({}),
        ))
        .await;

        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.viewer_id.as_str())
                .collect::<Vec<_>>(),
            vec!["200000001", "200000003"]
        );
    }

    #[tokio::test]
    async fn list_bans_turns_the_pagination_cursor_into_the_next_token() {
        let with_pagination = |pagination: Value| page_of(vec![row()], pagination);
        let cases = [
            (
                "cursor",
                with_pagination(json!({ "cursor": "eyJiIjpudWxsfQ" })),
                Some(BanPageToken::new("eyJiIjpudWxsfQ")),
            ),
            (
                "empty cursor",
                with_pagination(json!({ "cursor": "" })),
                None,
            ),
            (
                "null cursor",
                with_pagination(json!({ "cursor": null })),
                None,
            ),
            ("no cursor", with_pagination(json!({})), None),
            ("no pagination", json!({ "data": [row()] }), None),
        ];

        for (label, body, expected) in cases {
            let page = page_for(body).await;

            assert_eq!(page.next, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn list_bans_asks_for_full_pages_of_this_broadcaster() {
        let (transport, bundle) = answering(Ok(page_of(vec![], json!({}))));

        bundle.list_bans(None).await;

        let request = transport.last_request();
        assert_eq!(
            (
                request.method,
                request.path.as_str(),
                query_value(&request, "broadcaster_id"),
                query_value(&request, "first"),
            ),
            (
                HelixMethod::Get,
                "/helix/moderation/banned",
                Some(BROADCASTER.to_owned()),
                Some("100".to_owned()),
            )
        );
    }

    #[tokio::test]
    async fn list_bans_sends_after_only_for_a_non_empty_token() {
        let cases = [
            ("first page", None, None),
            ("empty token", Some(BanPageToken::new("")), None),
            (
                "next page",
                Some(BanPageToken::new("eyJiIjpudWxsfQ")),
                Some("eyJiIjpudWxsfQ".to_owned()),
            ),
        ];

        for (label, token, expected_after) in cases {
            let (transport, bundle) = answering(Ok(page_of(vec![], json!({}))));

            bundle.list_bans(token.as_ref()).await;

            assert_eq!(
                query_value(&transport.last_request(), "after"),
                expected_after,
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn list_bans_without_a_broadcaster_is_not_connected_and_sends_nothing() {
        let transport = Arc::new(MockTransport::returning(Ok(page_of(
            vec![row()],
            json!({}),
        ))));
        let bundle = bundle(Arc::clone(&transport) as Arc<dyn HelixTransport>, "");

        let outcome = bundle.list_bans(None).await;

        assert_eq!(
            (outcome, transport.call_count()),
            (
                BanListOutcome::Unavailable(BanListUnavailable::NotConnected),
                0
            )
        );
    }

    #[tokio::test]
    async fn list_bans_maps_each_api_failure_to_its_unavailability() {
        let http = |status: reqwest::StatusCode| HelixError::Http {
            status: status.as_u16(),
            body: String::new(),
        };
        let cases = [
            (
                "no credentials",
                HelixError::Credentials("no twitch credentials".to_owned()),
                BanListUnavailable::NotConnected,
            ),
            (
                "rejected after refresh",
                HelixError::ReauthRequired,
                BanListUnavailable::MissingScope,
            ),
            (
                "403",
                http(reqwest::StatusCode::FORBIDDEN),
                BanListUnavailable::MissingScope,
            ),
            (
                "400",
                http(reqwest::StatusCode::BAD_REQUEST),
                BanListUnavailable::Transport,
            ),
            (
                "404",
                http(reqwest::StatusCode::NOT_FOUND),
                BanListUnavailable::Transport,
            ),
            (
                "500",
                http(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
                BanListUnavailable::Transport,
            ),
            (
                "503",
                http(reqwest::StatusCode::SERVICE_UNAVAILABLE),
                BanListUnavailable::Transport,
            ),
            (
                "rate limited",
                HelixError::RateLimited,
                BanListUnavailable::Transport,
            ),
            (
                "transport",
                HelixError::Transport("connection refused".to_owned()),
                BanListUnavailable::Transport,
            ),
        ];

        for (label, error, expected) in cases {
            let (_transport, bundle) = answering(Err(error));

            assert_eq!(
                bundle.list_bans(None).await,
                BanListOutcome::Unavailable(expected),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn list_bans_is_a_transport_failure_when_the_body_is_not_a_banned_users_page() {
        let cases = [
            ("null", Value::Null),
            ("data is not a list", json!({ "data": "nope" })),
            ("row is not an object", json!({ "data": [42] })),
            (
                "cursor is not a string",
                page_of(vec![], json!({ "cursor": 7 })),
            ),
        ];

        for (label, body) in cases {
            let (_transport, bundle) = answering(Ok(body));

            assert_eq!(
                bundle.list_bans(None).await,
                BanListOutcome::Unavailable(BanListUnavailable::Transport),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn list_bans_is_missing_scope_when_the_refreshed_token_is_rejected_too() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/helix/moderation/banned"))
            .respond_with(ResponseTemplate::new(
                reqwest::StatusCode::UNAUTHORIZED.as_u16(),
            ))
            .mount(&server)
            .await;
        let refresher = Arc::new(CountingRefresher::default());
        let bundle = over_http(&server, Arc::clone(&refresher));

        let outcome = bundle.list_bans(None).await;

        assert_eq!(
            (
                outcome,
                refresher.refreshes.load(Ordering::SeqCst),
                server.received_requests().await.unwrap().len(),
            ),
            (
                BanListOutcome::Unavailable(BanListUnavailable::MissingScope),
                1,
                2
            ),
        );
    }
}
