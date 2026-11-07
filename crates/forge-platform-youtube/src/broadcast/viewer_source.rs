use forge_platform_core::{LiveViewerSource, ViewerReport, ViewerReportStream};
use tokio::sync::watch;
use tokio_stream::wrappers::WatchStream;

pub(crate) struct YouTubeViewerSource {
    reports: watch::Receiver<ViewerReport>,
}

impl YouTubeViewerSource {
    pub(crate) fn new(reports: watch::Receiver<ViewerReport>) -> Self {
        Self { reports }
    }
}

impl LiveViewerSource for YouTubeViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}
