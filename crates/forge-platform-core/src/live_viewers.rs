use std::pin::Pin;

use futures_core::Stream;
use serde::{Deserialize, Serialize};

/// A platform's current concurrent-viewer figure. `Live` carries an exact count
/// that may be zero; `Absent` means connected-but-not-live or no viewer concept
/// and is never conflated with a zero count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerReport {
    Live { count: u64 },
    Absent,
}

pub type ViewerReportStream = Pin<Box<dyn Stream<Item = ViewerReport> + Send + 'static>>;

pub trait LiveViewerSource: Send + Sync {
    /// The platform's concurrent-viewer figures over time. Latest-value-wins: a
    /// consumer tracking only the current figure may drop intermediate items.
    /// Ending the stream drops this platform's contribution, as does an
    /// `Absent` report.
    fn viewer_reports(&self) -> ViewerReportStream;
}
