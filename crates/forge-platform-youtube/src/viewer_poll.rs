use std::sync::Arc;
use std::time::Duration;

use forge_net::{GatewayFailure, GatewayRequest};
use forge_platform_core::{LiveViewerSource, PlatformError, ViewerReport, ViewerReportStream};
use forge_types::Secret;
use futures::future::BoxFuture;
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::data_api::{DataApiError, YoutubeDataApi};

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const VIEWER_LIST_COST: u32 = 1;
const VIDEOS_PATH: &str = "/videos";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

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
        loop {
            ticker.tick().await;
            if let Some(report) = self.poll_once().await {
                self.reports_tx.send_replace(report);
            }
        }
    }

    async fn poll_once(&self) -> Option<ViewerReport> {
        let video_id = match self.active_broadcast_id.get() {
            Some(id) => id,
            None => return Some(ViewerReport::Absent),
        };

        let token = (self.access_token_source)().await.ok()?;
        let request = GatewayRequest::get(VIDEOS_PATH)
            .label(VIDEOS_PATH)
            .bearer(Secret::new(token))
            .query("part", "liveStreamingDetails")
            .query("id", video_id);

        let response = match self.api.send(VIEWER_LIST_COST, request).await {
            Ok(response) => response,
            Err(DataApiError::Gateway(GatewayFailure::Transport(failure))) => {
                tracing::warn!("youtube viewer poll request failed: {failure}");
                return None;
            }
            Err(_) => return None,
        };

        let body: serde_json::Value = response.json().ok()?;
        Some(match extract_concurrent_viewers(&body) {
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

        let poll = make_poll(data_api_at(&server.uri()), Some("vid123".to_owned()));
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

        let poll = make_poll(data_api_at(&server.uri()), Some("vid123".to_owned()));
        assert_eq!(poll.poll_once().await, None);
    }

    #[tokio::test]
    async fn poll_once_reports_absent_without_active_broadcast() {
        let poll = make_poll(unreachable_data_api(), None);
        assert_eq!(poll.poll_once().await, Some(ViewerReport::Absent));
    }
}
