use std::pin::Pin;

use futures_core::Stream;
use serde::{Deserialize, Serialize};

/// `Absent` (no viewer concept / not live) is never conflated with a `Live { count: 0 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerReport {
    Live { count: u64 },
    Absent,
}

pub type ViewerReportStream = Pin<Box<dyn Stream<Item = ViewerReport> + Send + 'static>>;

pub trait LiveViewerSource: Send + Sync {
    /// Latest-value-wins; ending the stream drops this platform's contribution, as does
    /// an `Absent` report.
    fn viewer_reports(&self) -> ViewerReportStream;
}
