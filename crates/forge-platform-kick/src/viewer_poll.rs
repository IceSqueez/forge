use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{LiveViewerSource, RateLimiter, ViewerReport, ViewerReportStream};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;
use tracing::debug;

use crate::channel::KickChannel;
use crate::poller::TokenSource;

const POLL_INTERVAL: Duration = Duration::from_secs(45);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

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
    channel: KickChannel,
    token_source: TokenSource,
    reports_tx: watch::Sender<ViewerReport>,
}

impl KickViewerPoll {
    pub fn new(
        rate_limiter: Arc<dyn RateLimiter>,
        token_source: TokenSource,
    ) -> (Self, KickViewerSource) {
        Self::with_channel(KickChannel::new(rate_limiter), token_source)
    }

    fn with_channel(channel: KickChannel, token_source: TokenSource) -> (Self, KickViewerSource) {
        let (reports_tx, reports_rx) = watch::channel(ViewerReport::Absent);
        let poll = Self {
            channel,
            token_source,
            reports_tx,
        };
        (poll, KickViewerSource::new(reports_rx))
    }

    #[cfg(test)]
    pub(crate) fn with_api_base(
        rate_limiter: Arc<dyn RateLimiter>,
        token_source: TokenSource,
        api_base: String,
    ) -> (Self, KickViewerSource) {
        Self::with_channel(
            KickChannel::new(rate_limiter).with_api_base(api_base),
            token_source,
        )
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

    /// `None` is a transient miss whose last known figure must be kept, not erased.
    async fn poll_once(&self) -> Option<ViewerReport> {
        let token = (self.token_source)().await.ok()?;

        match tokio::time::timeout(HTTP_TIMEOUT, self.channel.get_channel(&token)).await {
            Ok(Ok(snapshot)) if snapshot.is_live => Some(ViewerReport::Live {
                count: snapshot.viewer_count,
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
    use forge_platform_core::{PlatformError, RateLimitOutcome};
    use futures::future::BoxFuture;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct GrantLimiter;
    #[async_trait::async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        fn remaining(&self) -> u32 {
            120
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn ok_token() -> TokenSource {
        Arc::new(|| {
            Box::pin(async { Ok::<_, PlatformError>("tok".to_owned()) }) as BoxFuture<'static, _>
        })
    }

    fn failing_token() -> TokenSource {
        Arc::new(|| {
            Box::pin(async {
                Err::<String, _>(PlatformError::ReauthRequired {
                    platform: "kick".to_owned(),
                })
            }) as BoxFuture<'static, _>
        })
    }

    fn channel_body(is_live: bool, viewer_count: u64) -> serde_json::Value {
        json!({
            "data": [{
                "is_live": is_live,
                "stream_title": "playing",
                "category": { "id": 1, "name": "Just Chatting" },
                "stream": { "viewer_count": viewer_count }
            }]
        })
    }

    async fn poll_against(response: ResponseTemplate) -> Option<ViewerReport> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(response)
            .mount(&server)
            .await;
        let (poll, _source) =
            KickViewerPoll::with_api_base(Arc::new(GrantLimiter), ok_token(), server.uri());
        poll.poll_once().await
    }

    #[tokio::test]
    async fn poll_once_reports_live_count_when_channel_is_live() {
        assert_eq!(
            poll_against(ResponseTemplate::new(200).set_body_json(channel_body(true, 500))).await,
            Some(ViewerReport::Live { count: 500 })
        );
    }

    #[tokio::test]
    async fn poll_once_reports_live_zero_not_absent_for_genuine_zero_viewers() {
        assert_eq!(
            poll_against(ResponseTemplate::new(200).set_body_json(channel_body(true, 0))).await,
            Some(ViewerReport::Live { count: 0 })
        );
    }

    #[tokio::test]
    async fn poll_once_reports_absent_when_offline_even_with_a_stale_viewer_count() {
        assert_eq!(
            poll_against(ResponseTemplate::new(200).set_body_json(channel_body(false, 300))).await,
            Some(ViewerReport::Absent)
        );
    }

    #[tokio::test]
    async fn poll_once_returns_none_on_non_200_keeping_last_figure() {
        assert_eq!(poll_against(ResponseTemplate::new(500)).await, None);
    }

    #[tokio::test]
    async fn poll_once_returns_none_without_calling_the_api_when_the_token_source_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, 42)))
            .mount(&server)
            .await;
        let (poll, _source) =
            KickViewerPoll::with_api_base(Arc::new(GrantLimiter), failing_token(), server.uri());

        assert_eq!(poll.poll_once().await, None);
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an unusable token must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn poll_once_authorizes_the_channels_request_with_the_token_source_value() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, 7)))
            .mount(&server)
            .await;
        let (poll, _source) =
            KickViewerPoll::with_api_base(Arc::new(GrantLimiter), ok_token(), server.uri());

        assert_eq!(
            poll.poll_once().await,
            Some(ViewerReport::Live { count: 7 })
        );
    }
}
