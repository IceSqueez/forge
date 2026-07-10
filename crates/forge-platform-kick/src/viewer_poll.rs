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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn poll_against(slug: &str, response: ResponseTemplate) -> Option<ViewerReport> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/{slug}")))
            .respond_with(response)
            .mount(&server)
            .await;
        let (poll, _source) =
            KickViewerPoll::with_endpoint(slug.to_owned(), reqwest::Client::new(), server.uri());
        poll.poll_once().await
    }

    #[tokio::test]
    async fn poll_once_reports_live_count_when_channel_is_live() {
        let body = json!({
            "chatroom": { "id": 1 },
            "livestream": { "viewer_count": 500, "session_title": "playing" }
        });
        assert_eq!(
            poll_against("live_slug", ResponseTemplate::new(200).set_body_json(body)).await,
            Some(ViewerReport::Live { count: 500 })
        );
    }

    #[tokio::test]
    async fn poll_once_reports_live_zero_not_absent_for_genuine_zero_viewers() {
        // A live channel with zero viewers is Live { 0 }, never Absent.
        let body = json!({
            "chatroom": { "id": 1 },
            "livestream": { "viewer_count": 0, "session_title": "starting soon" }
        });
        assert_eq!(
            poll_against("zero_slug", ResponseTemplate::new(200).set_body_json(body)).await,
            Some(ViewerReport::Live { count: 0 })
        );
    }

    #[tokio::test]
    async fn poll_once_reports_absent_when_channel_is_offline() {
        let body = json!({ "chatroom": { "id": 1 }, "livestream": null });
        assert_eq!(
            poll_against(
                "offline_slug",
                ResponseTemplate::new(200).set_body_json(body)
            )
            .await,
            Some(ViewerReport::Absent)
        );
    }

    #[tokio::test]
    async fn poll_once_returns_none_on_non_200_keeping_last_figure() {
        // A transient miss must not erase the last figure to Absent or Live { 0 }.
        assert_eq!(
            poll_against("err_slug", ResponseTemplate::new(500)).await,
            None
        );
    }
}
