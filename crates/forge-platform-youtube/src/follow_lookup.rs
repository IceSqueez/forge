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
use tokio::sync::Mutex;

use crate::builtin::YoutubeIntegrationBundle;
use crate::quota_state::{QuotaState, today_pacific};

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
    quota: Arc<Mutex<QuotaState>>,
    rate_limiter: Arc<dyn RateLimiter>,
    api_base: String,
    broadcaster_channel_id: String,
}

impl YoutubeFollowLookup {
    pub(crate) fn new(
        endpoints: &PlatformEndpoints,
        access_token_source: TokenSource,
        quota: Arc<Mutex<QuotaState>>,
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
