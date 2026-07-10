use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    LiveViewerSource, RateLimitOutcome, RateLimiter, TokenBucketRateLimiter, ViewerReport,
    ViewerReportStream,
};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tracing::debug;

use crate::channel_info::ChannelInfoFetcher;

const POLL_INTERVAL: Duration = Duration::from_secs(45);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// A standalone bucket bounding this poll's own request rate so a tick storm can
/// never burst the unofficial channel endpoint; the real ceiling is the shared
/// per-client estimate honored by the official-API polls.
const READ_BUDGET_CAPACITY: u32 = 4;
const READ_BUDGET_WINDOW: Duration = Duration::from_secs(60);
const VIEWER_POLL_COST: u32 = 1;

pub struct KickViewerSource {
    reports: watch::Receiver<ViewerReport>,
}

impl KickViewerSource {
    fn new(reports: watch::Receiver<ViewerReport>) -> Self {
        Self { reports }
    }
}

impl LiveViewerSource for KickViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}

pub struct KickViewerPoll {
    fetcher: ChannelInfoFetcher,
    rate_limiter: Arc<dyn RateLimiter>,
    reports_tx: watch::Sender<ViewerReport>,
}

impl KickViewerPoll {
    pub fn new(slug: String, http: reqwest::Client) -> (Self, KickViewerSource) {
        Self::with_fetcher(ChannelInfoFetcher::new(slug, http))
    }

    fn with_fetcher(fetcher: ChannelInfoFetcher) -> (Self, KickViewerSource) {
        let (reports_tx, reports_rx) = watch::channel(ViewerReport::Absent);
        let poll = Self {
            fetcher,
            rate_limiter: Arc::new(TokenBucketRateLimiter::new(
                READ_BUDGET_CAPACITY,
                READ_BUDGET_WINDOW,
            )),
            reports_tx,
        };
        (poll, KickViewerSource::new(reports_rx))
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_endpoint(
        slug: String,
        http: reqwest::Client,
        endpoint_base: String,
    ) -> (Self, KickViewerSource) {
        Self::with_fetcher(ChannelInfoFetcher::with_endpoint(slug, http, endpoint_base))
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

    /// `Some(Live)` / `Some(Absent)` is a definitive figure to publish; `None` is
    /// a transient miss (throttle, network, timeout, non-200) whose last known
    /// figure must be kept rather than erased to zero or absence.
    async fn poll_once(&self) -> Option<ViewerReport> {
        match self.rate_limiter.acquire(VIEWER_POLL_COST).await {
            Ok(RateLimitOutcome::Granted) => {}
            _ => return None,
        }

        match tokio::time::timeout(HTTP_TIMEOUT, self.fetcher.fetch()).await {
            Ok(Ok(info)) if info.is_live => Some(ViewerReport::Live {
                count: info.viewer_count,
            }),
            Ok(Ok(_)) => Some(ViewerReport::Absent),
            Ok(Err(error)) => {
                debug!(%error, "kick viewer poll: channel info unavailable");
                None
            }
            Err(_) => {
                debug!("kick viewer poll timed out");
                None
            }
        }
    }
}
