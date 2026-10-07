use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_platform_core::{
    EndpointSurface, FollowLookup, FollowStatus, PlatformEndpoints, PlatformError,
    RateLimitOutcome, RateLimiter, TokenBucketRateLimiter,
};
use futures::future::BoxFuture;
use reqwest::StatusCode;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::builtin::YoutubeIntegrationBundle;
use crate::quota_state::{SharedQuota, today_pacific};

const SUBSCRIPTION_LIST_COST: u32 = 1;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const READ_BUDGET_CAPACITY: u32 = 60;
const READ_BUDGET_WINDOW: Duration = Duration::from_secs(60);
const SUBSCRIPTIONS_PATH: &str = "/subscriptions";
const PART_PARAM: &str = "part";
const SNIPPET_PART: &str = "snippet";
const CHANNEL_ID_PARAM: &str = "channelId";
const FOR_CHANNEL_ID_PARAM: &str = "forChannelId";
const SUBSCRIPTION_FORBIDDEN_REASON: &str = "subscriptionForbidden";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

#[derive(Deserialize)]
struct SubscriptionPage {
    #[serde(default)]
    items: Vec<SubscriptionItem>,
}

#[derive(Deserialize)]
struct SubscriptionItem {
    snippet: Option<SubscriptionSnippet>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionSnippet {
    published_at: Option<String>,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    errors: Vec<ErrorDetail>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    reason: Option<String>,
}

pub(crate) struct YoutubeFollowLookup {
    client: reqwest::Client,
    access_token_source: TokenSource,
    quota: SharedQuota,
    rate_limiter: Arc<dyn RateLimiter>,
    api_base: String,
    broadcaster_channel_id: String,
}

impl YoutubeFollowLookup {
    pub(crate) fn new(
        endpoints: &PlatformEndpoints,
        access_token_source: TokenSource,
        quota: SharedQuota,
        broadcaster_channel_id: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            quota,
            rate_limiter: Arc::new(TokenBucketRateLimiter::new(
                READ_BUDGET_CAPACITY,
                READ_BUDGET_WINDOW,
            )),
            api_base: endpoints
                .base_url(EndpointSurface::YouTubeDataApi)
                .to_owned(),
            broadcaster_channel_id,
        }
    }

    pub(crate) async fn status_of(&self, viewer_channel_id: &str) -> FollowStatus {
        let broadcaster_channel_id = self.broadcaster_channel_id.as_str();
        if broadcaster_channel_id.is_empty() || viewer_channel_id.is_empty() {
            return FollowStatus::Unavailable;
        }
        if viewer_channel_id == broadcaster_channel_id {
            return FollowStatus::NotFollowing;
        }
        if !matches!(
            self.rate_limiter.acquire(SUBSCRIPTION_LIST_COST).await,
            Ok(RateLimitOutcome::Granted)
        ) {
            return FollowStatus::Unavailable;
        }
        if self.charge_quota().await.is_err() {
            return FollowStatus::Unavailable;
        }
        let Ok(token) = (self.access_token_source)().await else {
            return FollowStatus::Unavailable;
        };

        let exchange = self.list_subscription(&token, viewer_channel_id, broadcaster_channel_id);
        match tokio::time::timeout(HTTP_TIMEOUT, exchange).await {
            Ok(Ok((status, body))) => status_from_reply(status, &body),
            Ok(Err(e)) => {
                tracing::warn!(error = %e.without_url(), "youtube follow lookup request failed");
                FollowStatus::Unavailable
            }
            Err(_) => {
                tracing::warn!("youtube follow lookup timed out");
                FollowStatus::Unavailable
            }
        }
    }

    async fn charge_quota(&self) -> Result<(), PlatformError> {
        let today = today_pacific();
        let mut quota = self.quota.lock().await;
        quota.charge(SUBSCRIPTION_LIST_COST, today)
    }

    async fn list_subscription(
        &self,
        token: &str,
        viewer_channel_id: &str,
        broadcaster_channel_id: &str,
    ) -> Result<(StatusCode, String), reqwest::Error> {
        let response = self
            .client
            .get(format!("{}{SUBSCRIPTIONS_PATH}", self.api_base))
            .bearer_auth(token)
            .query(&[
                (PART_PARAM, SNIPPET_PART),
                (CHANNEL_ID_PARAM, viewer_channel_id),
                (FOR_CHANNEL_ID_PARAM, broadcaster_channel_id),
            ])
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        Ok((status, body))
    }
}

fn status_from_reply(status: StatusCode, body: &str) -> FollowStatus {
    match status {
        StatusCode::OK => serde_json::from_str::<SubscriptionPage>(body)
            .map_or(FollowStatus::Unavailable, status_from_page),
        StatusCode::FORBIDDEN if carries_error_reason(body, SUBSCRIPTION_FORBIDDEN_REASON) => {
            FollowStatus::Hidden
        }
        _ => {
            tracing::debug!(status = status.as_u16(), "youtube follow lookup refused");
            FollowStatus::Unavailable
        }
    }
}

fn status_from_page(page: SubscriptionPage) -> FollowStatus {
    let Some(item) = page.items.into_iter().next() else {
        return FollowStatus::NotFollowing;
    };
    item.snippet
        .and_then(|snippet| snippet.published_at)
        .and_then(|raw| OffsetDateTime::parse(&raw, &Rfc3339).ok())
        .map_or(FollowStatus::Unavailable, FollowStatus::FollowedSince)
}

fn carries_error_reason(body: &str, wanted: &str) -> bool {
    serde_json::from_str::<ErrorEnvelope>(body).is_ok_and(|envelope| {
        envelope
            .error
            .errors
            .iter()
            .any(|detail| detail.reason.as_deref() == Some(wanted))
    })
}

#[async_trait]
impl FollowLookup for YoutubeIntegrationBundle {
    async fn follow_status(&self, viewer_id: &str) -> FollowStatus {
        self.follow_lookup().status_of(viewer_id).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::ffi::OsString;

    use serde_json::{Value, json};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::quota_state::QuotaState;

    const BROADCASTER: &str = "UCbroadcasterchannel0001";
    const VIEWER: &str = "UCviewerchannelid0000042";
    const ACCESS_TOKEN: &str = "yt-follow-secret-token";
    const SUBSCRIBED_AT: &str = "2023-04-05T06:07:08Z";
    const SUBSCRIBED_AT_UNIX: i64 = 1_680_674_828;
    const DAILY_LIMIT: u32 = 10_000;
    const LOCAL_READ_BUDGET: usize = 60;

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(ACCESS_TOKEN.to_owned()) }))
    }

    fn data_api_at(base: &str) -> PlatformEndpoints {
        PlatformEndpoints::resolve(|variable| {
            (variable == EndpointSurface::YouTubeDataApi.env_var()).then(|| OsString::from(base))
        })
        .unwrap()
    }

    fn fresh_quota() -> SharedQuota {
        SharedQuota::default()
    }

    fn exhausted_quota() -> SharedQuota {
        SharedQuota::new(QuotaState {
            used_today: DAILY_LIMIT,
            peak_seen: DAILY_LIMIT,
            last_reset_date: today_pacific(),
            long_interval_mode: true,
        })
    }

    fn lookup_at(base: &str, quota: &SharedQuota, broadcaster: &str) -> YoutubeFollowLookup {
        YoutubeFollowLookup::new(
            &data_api_at(base),
            token_source(),
            quota.clone(),
            broadcaster.to_owned(),
        )
    }

    async fn used_units(quota: &SharedQuota) -> u32 {
        quota.lock().await.used_today
    }

    async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
    }

    async fn server_answering(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/subscriptions"))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    fn ok_json(body: Value) -> ResponseTemplate {
        ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(body)
    }

    fn subscription_page(published_at: Value) -> Value {
        json!({
            "kind": "youtube#subscriptionListResponse",
            "pageInfo": { "totalResults": 1, "resultsPerPage": 5 },
            "items": [{
                "kind": "youtube#subscription",
                "id": "subscription-id",
                "snippet": {
                    "publishedAt": published_at,
                    "title": "Broadcaster",
                    "resourceId": { "kind": "youtube#channel", "channelId": BROADCASTER },
                    "channelId": VIEWER
                }
            }]
        })
    }

    fn google_error(status: StatusCode, reasons: &[&str]) -> ResponseTemplate {
        let errors: Vec<Value> = reasons
            .iter()
            .map(|reason| json!({ "domain": "youtube.subscription", "reason": reason, "message": "refused" }))
            .collect();
        ResponseTemplate::new(status.as_u16()).set_body_json(json!({
            "error": { "code": status.as_u16(), "message": "refused", "errors": errors }
        }))
    }

    fn subscribed_at() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(SUBSCRIBED_AT_UNIX).unwrap()
    }

    fn unreachable_base() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{address}")
    }

    #[tokio::test]
    async fn status_of_asks_the_data_api_for_the_viewer_subscription_to_the_broadcaster() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/subscriptions"))
            .and(query_param("part", "snippet"))
            .and(query_param("channelId", VIEWER))
            .and(query_param("forChannelId", BROADCASTER))
            .and(header("authorization", format!("Bearer {ACCESS_TOKEN}")))
            .respond_with(ok_json(subscription_page(json!(SUBSCRIBED_AT))))
            .mount(&server)
            .await;
        let lookup = lookup_at(&server.uri(), &fresh_quota(), BROADCASTER);

        assert_eq!(
            lookup.status_of(VIEWER).await,
            FollowStatus::FollowedSince(subscribed_at())
        );
    }

    #[tokio::test]
    async fn a_successful_lookup_charges_exactly_one_quota_unit() {
        let server = server_answering(ok_json(subscription_page(json!(SUBSCRIBED_AT)))).await;
        let quota = fresh_quota();
        let lookup = lookup_at(&server.uri(), &quota, BROADCASTER);

        lookup.status_of(VIEWER).await;

        assert_eq!(used_units(&quota).await, 1);
    }

    #[tokio::test]
    async fn status_of_maps_each_subscription_page_shape() {
        let cases = [
            (
                "offset instant",
                ok_json(subscription_page(json!("2023-04-05T09:07:08+03:00"))),
                FollowStatus::FollowedSince(subscribed_at()),
            ),
            (
                "fractional seconds",
                ok_json(subscription_page(json!("2023-04-05T06:07:08.5Z"))),
                FollowStatus::FollowedSince(subscribed_at() + time::Duration::milliseconds(500)),
            ),
            (
                "empty items",
                ok_json(json!({ "pageInfo": { "totalResults": 0 }, "items": [] })),
                FollowStatus::NotFollowing,
            ),
            (
                "missing items",
                ok_json(json!({ "pageInfo": { "totalResults": 0 } })),
                FollowStatus::NotFollowing,
            ),
            (
                "item without snippet",
                ok_json(json!({ "items": [{ "id": "subscription-id" }] })),
                FollowStatus::Unavailable,
            ),
            (
                "snippet without publishedAt",
                ok_json(json!({ "items": [{ "snippet": { "title": "Broadcaster" } }] })),
                FollowStatus::Unavailable,
            ),
            (
                "null publishedAt",
                ok_json(subscription_page(Value::Null)),
                FollowStatus::Unavailable,
            ),
            (
                "empty publishedAt",
                ok_json(subscription_page(json!(""))),
                FollowStatus::Unavailable,
            ),
            (
                "malformed publishedAt",
                ok_json(subscription_page(json!("yesterday"))),
                FollowStatus::Unavailable,
            ),
            (
                "date without time",
                ok_json(subscription_page(json!("2023-04-05"))),
                FollowStatus::Unavailable,
            ),
            (
                "numeric publishedAt",
                ok_json(subscription_page(json!(SUBSCRIBED_AT_UNIX))),
                FollowStatus::Unavailable,
            ),
            (
                "non-json body",
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string("<html>"),
                FollowStatus::Unavailable,
            ),
            (
                "items is not a list",
                ok_json(json!({ "items": "none" })),
                FollowStatus::Unavailable,
            ),
        ];

        for (label, response, expected) in cases {
            let server = server_answering(response).await;
            let lookup = lookup_at(&server.uri(), &fresh_quota(), BROADCASTER);

            assert_eq!(lookup.status_of(VIEWER).await, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn status_of_maps_each_refusal_by_status_and_reason() {
        let cases = [
            (
                "403 subscriptionForbidden",
                google_error(StatusCode::FORBIDDEN, &["subscriptionForbidden"]),
                FollowStatus::Hidden,
            ),
            (
                "403 quotaExceeded then subscriptionForbidden",
                google_error(
                    StatusCode::FORBIDDEN,
                    &["quotaExceeded", "subscriptionForbidden"],
                ),
                FollowStatus::Hidden,
            ),
            (
                "403 subscriptionForbidden then quotaExceeded",
                google_error(
                    StatusCode::FORBIDDEN,
                    &["subscriptionForbidden", "quotaExceeded"],
                ),
                FollowStatus::Hidden,
            ),
            (
                "403 quotaExceeded",
                google_error(StatusCode::FORBIDDEN, &["quotaExceeded"]),
                FollowStatus::Unavailable,
            ),
            (
                "403 without errors",
                google_error(StatusCode::FORBIDDEN, &[]),
                FollowStatus::Unavailable,
            ),
            (
                "403 non-json body",
                ResponseTemplate::new(StatusCode::FORBIDDEN.as_u16()).set_body_string("denied"),
                FollowStatus::Unavailable,
            ),
            (
                "401 subscriptionForbidden",
                google_error(StatusCode::UNAUTHORIZED, &["subscriptionForbidden"]),
                FollowStatus::Unavailable,
            ),
            (
                "401 authError",
                google_error(StatusCode::UNAUTHORIZED, &["authError"]),
                FollowStatus::Unavailable,
            ),
            (
                "404 subscriberNotFound",
                google_error(StatusCode::NOT_FOUND, &["subscriberNotFound"]),
                FollowStatus::Unavailable,
            ),
            (
                "500 backendError",
                google_error(StatusCode::INTERNAL_SERVER_ERROR, &["backendError"]),
                FollowStatus::Unavailable,
            ),
            (
                "503 without body",
                ResponseTemplate::new(StatusCode::SERVICE_UNAVAILABLE.as_u16()),
                FollowStatus::Unavailable,
            ),
        ];

        for (label, response, expected) in cases {
            let server = server_answering(response).await;
            let lookup = lookup_at(&server.uri(), &fresh_quota(), BROADCASTER);

            assert_eq!(lookup.status_of(VIEWER).await, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn status_of_is_unavailable_when_the_data_api_host_is_unreachable() {
        let lookup = lookup_at(&unreachable_base(), &fresh_quota(), BROADCASTER);

        assert_eq!(lookup.status_of(VIEWER).await, FollowStatus::Unavailable);
    }

    #[tokio::test]
    async fn status_of_is_unavailable_when_the_data_api_never_answers() {
        let server = server_answering(
            ok_json(subscription_page(json!(SUBSCRIBED_AT))).set_delay(Duration::from_secs(600)),
        )
        .await;
        let lookup = lookup_at(&server.uri(), &fresh_quota(), BROADCASTER);
        tokio::time::pause();
        let started = tokio::time::Instant::now();

        let status = lookup.status_of(VIEWER).await;

        assert_eq!(
            (status, started.elapsed() < Duration::from_secs(60)),
            (FollowStatus::Unavailable, true)
        );
    }

    #[tokio::test]
    async fn status_of_is_unavailable_when_the_token_source_fails() {
        let server = server_answering(ok_json(subscription_page(json!(SUBSCRIBED_AT)))).await;
        let failing: TokenSource = Arc::new(|| {
            Box::pin(async {
                Err(PlatformError::ReauthRequired {
                    platform: "youtube".to_owned(),
                })
            })
        });
        let lookup = YoutubeFollowLookup::new(
            &data_api_at(&server.uri()),
            failing,
            fresh_quota(),
            BROADCASTER.to_owned(),
        );

        assert_eq!(
            (lookup.status_of(VIEWER).await, request_count(&server).await),
            (FollowStatus::Unavailable, 0)
        );
    }

    #[tokio::test]
    async fn a_missing_broadcaster_or_viewer_id_is_unavailable_without_a_request_or_charge() {
        for (broadcaster, viewer) in [("", VIEWER), (BROADCASTER, ""), ("", "")] {
            let server = server_answering(ok_json(subscription_page(json!(SUBSCRIBED_AT)))).await;
            let quota = fresh_quota();
            let lookup = lookup_at(&server.uri(), &quota, broadcaster);

            let status = lookup.status_of(viewer).await;

            assert_eq!(
                (
                    status,
                    request_count(&server).await,
                    used_units(&quota).await
                ),
                (FollowStatus::Unavailable, 0, 0),
                "broadcaster {broadcaster:?} / viewer {viewer:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_broadcaster_looking_up_themself_is_not_following_without_a_request_or_charge() {
        let server = server_answering(ok_json(subscription_page(json!(SUBSCRIBED_AT)))).await;
        let quota = fresh_quota();
        let lookup = lookup_at(&server.uri(), &quota, BROADCASTER);

        let status = lookup.status_of(BROADCASTER).await;

        assert_eq!(
            (
                status,
                request_count(&server).await,
                used_units(&quota).await
            ),
            (FollowStatus::NotFollowing, 0, 0)
        );
    }

    #[tokio::test]
    async fn an_exhausted_daily_quota_is_unavailable_without_a_request() {
        let server = server_answering(ok_json(subscription_page(json!(SUBSCRIBED_AT)))).await;
        let quota = exhausted_quota();
        let lookup = lookup_at(&server.uri(), &quota, BROADCASTER);

        let status = lookup.status_of(VIEWER).await;

        assert_eq!(
            (
                status,
                request_count(&server).await,
                used_units(&quota).await
            ),
            (FollowStatus::Unavailable, 0, DAILY_LIMIT)
        );
    }

    #[tokio::test]
    async fn a_spent_local_read_budget_is_unavailable_without_a_request_or_charge() {
        let server = server_answering(ok_json(subscription_page(json!(SUBSCRIBED_AT)))).await;
        let quota = fresh_quota();
        let lookup = lookup_at(&server.uri(), &quota, BROADCASTER);
        for _ in 0..LOCAL_READ_BUDGET {
            lookup.status_of(VIEWER).await;
        }

        let status = lookup.status_of(VIEWER).await;

        assert_eq!(
            (
                status,
                request_count(&server).await,
                used_units(&quota).await
            ),
            (
                FollowStatus::Unavailable,
                LOCAL_READ_BUDGET,
                u32::try_from(LOCAL_READ_BUDGET).unwrap()
            )
        );
    }
}
