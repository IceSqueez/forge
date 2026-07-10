use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    LiveViewerSource, PlatformError, RateLimitOutcome, RateLimiter, TokenBucketRateLimiter,
    ViewerReport, ViewerReportStream,
};
use futures::future::BoxFuture;
use tokio::sync::{Mutex, watch};
use tokio_stream::wrappers::WatchStream;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const POLL_INTERVAL: Duration = Duration::from_secs(60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// `videos.list` costs one Data API unit per call; a live poll never runs while
/// no broadcast is active, so the ceiling is one unit per `POLL_INTERVAL`.
const VIEWER_LIST_COST: u32 = 1;

/// A bucket bounding this poll's own request rate. The real Data API ceiling is
/// the shared daily 10k-unit quota tracked in `QuotaState`; this only stops a
/// pathological tick storm from bursting past a modest read rate.
const READ_BUDGET_CAPACITY: u32 = 60;
const READ_BUDGET_WINDOW: Duration = Duration::from_secs(60);

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

/// Bridges the concurrent-viewer poll into the runtime live-viewer aggregate.
/// Holds only a `watch` receiver, so it never keeps the poll task alive.
pub struct YoutubeViewerSource {
    reports: watch::Receiver<ViewerReport>,
}

impl YoutubeViewerSource {
    pub(crate) fn new(reports: watch::Receiver<ViewerReport>) -> Self {
        Self { reports }
    }
}

impl LiveViewerSource for YoutubeViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}

pub struct YoutubeViewerPoll {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    rate_limiter: Arc<dyn RateLimiter>,
    reports_tx: watch::Sender<ViewerReport>,
    api_base: String,
}

impl YoutubeViewerPoll {
    pub fn new(
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
        quota: Arc<Mutex<QuotaState>>,
        reports_tx: watch::Sender<ViewerReport>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            active_broadcast_id,
            quota,
            rate_limiter: Arc::new(TokenBucketRateLimiter::new(
                READ_BUDGET_CAPACITY,
                READ_BUDGET_WINDOW,
            )),
            reports_tx,
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn run(self) {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Some(report) = self.poll_once().await {
                let _ = self.reports_tx.send(report);
            }
        }
    }

    /// One poll of the active broadcast's `liveStreamingDetails.concurrentViewers`.
    /// `Some(Live)` / `Some(Absent)` is a definitive figure to publish; `None`
    /// means a transient miss (throttle, quota, network, non-200) whose last
    /// known figure must be kept rather than erased to zero or absence.
    async fn poll_once(&self) -> Option<ViewerReport> {
        let video_id = match self.active_broadcast_id.get() {
            Some(id) => id,
            None => return Some(ViewerReport::Absent),
        };

        match self.rate_limiter.acquire(VIEWER_LIST_COST).await {
            Ok(RateLimitOutcome::Granted) => {}
            _ => return None,
        }

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            if qt.charge(VIEWER_LIST_COST, today).is_err() {
                return None;
            }
        }

        let token = (self.access_token_source)().await.ok()?;
        let url = format!("{}/videos", self.api_base);
        let request = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[("part", "liveStreamingDetails"), ("id", video_id.as_str())])
            .send();

        let resp = match tokio::time::timeout(HTTP_TIMEOUT, request).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => {
                tracing::warn!("youtube viewer poll request failed: {}", e.without_url());
                return None;
            }
            Err(_) => {
                tracing::warn!("youtube viewer poll timed out");
                return None;
            }
        };

        if resp.status().as_u16() != 200 {
            return None;
        }

        let body: serde_json::Value = resp.json().await.ok()?;
        Some(match extract_concurrent_viewers(&body) {
            Some(count) => ViewerReport::Live { count },
            None => ViewerReport::Absent,
        })
    }
}

/// Pulls `items[0].liveStreamingDetails.concurrentViewers` (an unsigned count
/// serialized as a string) from a `videos.list` body. Absent for a non-live,
/// ended, or hidden-count broadcast — never coerced to zero.
fn extract_concurrent_viewers(body: &serde_json::Value) -> Option<u64> {
    body.get("items")?
        .as_array()?
        .first()?
        .get("liveStreamingDetails")?
        .get("concurrentViewers")?
        .as_str()?
        .parse::<u64>()
        .ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_poll(api_base: String, video_id: Option<String>) -> YoutubeViewerPoll {
        let broadcast = ActiveBroadcastIdHandle::new();
        broadcast.set(video_id);
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let (tx, _rx) = watch::channel(ViewerReport::Absent);
        let source: TokenSource = Arc::new(|| {
            Box::pin(async { Ok("test-token".to_owned()) })
                as BoxFuture<'static, Result<String, PlatformError>>
        });
        YoutubeViewerPoll::new(source, broadcast, quota, tx).with_api_base(api_base)
    }

    #[test]
    fn extract_reads_concurrent_viewers_from_nested_string_field() {
        let body = json!({
            "items": [ { "liveStreamingDetails": { "concurrentViewers": "1234" } } ]
        });
        assert_eq!(extract_concurrent_viewers(&body), Some(1234));
    }

    #[test]
    fn extract_yields_none_for_every_absent_or_unparseable_shape() {
        // Each shape breaks the nested path at a different link; none may be
        // coerced to a zero count (that is the Absent/Live distinction).
        for (label, body) in [
            ("empty items", json!({ "items": [] })),
            ("missing liveStreamingDetails", json!({ "items": [ {} ] })),
            (
                "missing concurrentViewers",
                json!({ "items": [ { "liveStreamingDetails": {} } ] }),
            ),
            (
                "non-numeric count",
                json!({ "items": [ { "liveStreamingDetails": { "concurrentViewers": "many" } } ] }),
            ),
        ] {
            assert_eq!(extract_concurrent_viewers(&body), None, "{label}");
        }
    }

    #[tokio::test]
    async fn poll_once_reports_live_count_when_broadcast_has_viewers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [ { "liveStreamingDetails": { "concurrentViewers": "1234" } } ]
            })))
            .mount(&server)
            .await;

        let poll = make_poll(server.uri(), Some("vid123".to_owned()));
        assert_eq!(
            poll.poll_once().await,
            Some(ViewerReport::Live { count: 1234 })
        );
    }

    #[tokio::test]
    async fn poll_once_reports_absent_when_count_is_hidden_not_live_zero() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [ { "liveStreamingDetails": {} } ]
            })))
            .mount(&server)
            .await;

        let poll = make_poll(server.uri(), Some("vid123".to_owned()));
        assert_eq!(poll.poll_once().await, Some(ViewerReport::Absent));
    }

    #[tokio::test]
    async fn poll_once_returns_none_on_non_200_keeping_last_figure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let poll = make_poll(server.uri(), Some("vid123".to_owned()));
        assert_eq!(poll.poll_once().await, None);
    }

    #[tokio::test]
    async fn poll_once_reports_absent_without_active_broadcast() {
        // No active broadcast id: the poll short-circuits to Absent without a
        // request, never a transient None.
        let poll = make_poll("http://255.255.255.255".to_owned(), None);
        assert_eq!(poll.poll_once().await, Some(ViewerReport::Absent));
    }
}
