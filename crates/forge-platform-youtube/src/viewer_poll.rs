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
