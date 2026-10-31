use std::sync::Arc;
use std::time::Duration;

use forge_net::GatewayRequest;
use forge_platform_core::{LiveViewerSource, PlatformError, ViewerReport, ViewerReportStream};
use forge_types::Secret;
use futures::future::BoxFuture;
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::data_api::{YoutubeDataApi, malformed};
use crate::poll_health::CONSECUTIVE_POLL_FAILURES_FOR_LOSS;

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const VIEWER_LIST_COST: u32 = 1;
const VIDEOS_PATH: &str = "/videos";
const MISSING_SCOPE: &str = "viewer read scope missing";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub(crate) struct YoutubeViewerSource {
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
    api: YoutubeDataApi,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    reports_tx: watch::Sender<ViewerReport>,
}

impl YoutubeViewerPoll {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
        reports_tx: watch::Sender<ViewerReport>,
    ) -> Self {
        Self {
            api,
            access_token_source,
            active_broadcast_id,
            reports_tx,
        }
    }

    pub async fn run(self) {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut consecutive_failures: u32 = 0;
        loop {
            ticker.tick().await;
            match self.poll_once().await {
                Ok(report) => {
                    consecutive_failures = 0;
                    self.reports_tx.send_replace(report);
                }
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    tracing::warn!(
                        error = %crate::error_shape::redact_platform_error(&error),
                        consecutive_failures,
                        "youtube viewer poll failed"
                    );
                    if consecutive_failures >= CONSECUTIVE_POLL_FAILURES_FOR_LOSS {
                        self.reports_tx.send_replace(ViewerReport::Absent);
                    }
                }
            }
        }
    }

    async fn poll_once(&self) -> Result<ViewerReport, PlatformError> {
        let Some(video_id) = self.active_broadcast_id.get() else {
            return Ok(ViewerReport::Absent);
        };

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::get(VIDEOS_PATH)
            .label(VIDEOS_PATH)
            .bearer(Secret::new(token))
            .query("part", "liveStreamingDetails")
            .query("id", video_id);

        let response = self
            .api
            .send(VIEWER_LIST_COST, request)
            .await
            .map_err(|error| error.into_platform_error(MISSING_SCOPE))?;

        let body: serde_json::Value = response.json().map_err(malformed)?;
        Ok(match extract_concurrent_viewers(&body) {
            Some(count) => ViewerReport::Live { count },
            None => ViewerReport::Absent,
        })
    }
}

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
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::endpoints_test_support::{data_api_at, unreachable_data_api};

    fn make_poll(api: YoutubeDataApi, video_id: Option<String>) -> YoutubeViewerPoll {
        let broadcast = ActiveBroadcastIdHandle::new();
        broadcast.set(video_id);
        let (tx, _rx) = watch::channel(ViewerReport::Absent);
        let source: TokenSource = Arc::new(|| {
            Box::pin(async { Ok("test-token".to_owned()) })
                as BoxFuture<'static, Result<String, PlatformError>>
        });
        YoutubeViewerPoll::new(api, source, broadcast, tx)
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

        let poll = make_poll(data_api_at(&server.uri()), Some("vid123".to_owned()));
        assert!(matches!(
            poll.poll_once().await,
            Ok(ViewerReport::Live { count: 1234 })
        ));
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

        let poll = make_poll(data_api_at(&server.uri()), Some("vid123".to_owned()));
        assert!(matches!(poll.poll_once().await, Ok(ViewerReport::Absent)));
    }

    #[tokio::test]
    async fn poll_once_errors_on_non_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let poll = make_poll(data_api_at(&server.uri()), Some("vid123".to_owned()));
        assert!(poll.poll_once().await.is_err());
    }

    #[tokio::test]
    async fn poll_once_reports_absent_without_active_broadcast() {
        let poll = make_poll(unreachable_data_api(), None);
        assert!(matches!(poll.poll_once().await, Ok(ViewerReport::Absent)));
    }

    fn failing_poll(
        initial: ViewerReport,
    ) -> (
        watch::Receiver<ViewerReport>,
        tokio::sync::mpsc::UnboundedReceiver<()>,
        YoutubeViewerPoll,
    ) {
        let broadcast = ActiveBroadcastIdHandle::new();
        broadcast.set(Some("vid".to_owned()));
        let (tx, rx) = watch::channel(initial);
        let (attempt_tx, attempt_rx) = tokio::sync::mpsc::unbounded_channel();
        let source: TokenSource = Arc::new(move || {
            attempt_tx.send(()).unwrap();
            Box::pin(async {
                Err(PlatformError::Refused {
                    reason: "token unavailable".to_owned(),
                })
            }) as BoxFuture<'static, Result<String, PlatformError>>
        });
        let poll = YoutubeViewerPoll::new(unreachable_data_api(), source, broadcast, tx);
        (rx, attempt_rx, poll)
    }

    async fn await_attempts(attempts: &mut tokio::sync::mpsc::UnboundedReceiver<()>, count: usize) {
        for _ in 0..count {
            attempts.recv().await.unwrap();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn run_keeps_last_count_through_two_consecutive_failures() {
        let (rx, mut attempts, poll) = failing_poll(ViewerReport::Live { count: 42 });
        let task = tokio::spawn(poll.run());

        await_attempts(
            &mut attempts,
            CONSECUTIVE_POLL_FAILURES_FOR_LOSS as usize - 1,
        )
        .await;

        assert_eq!(*rx.borrow(), ViewerReport::Live { count: 42 });
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn run_reports_absent_after_three_consecutive_failures() {
        let (rx, mut attempts, poll) = failing_poll(ViewerReport::Live { count: 42 });
        let task = tokio::spawn(poll.run());

        await_attempts(&mut attempts, CONSECUTIVE_POLL_FAILURES_FOR_LOSS as usize).await;
        attempts.recv().await.unwrap();

        assert_eq!(*rx.borrow(), ViewerReport::Absent);
        task.abort();
    }
}
