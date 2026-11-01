use std::pin::Pin;

use futures_core::Stream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ViewerReport {
    Live { count: u64 },
    Absent,
}

pub type ViewerReportStream = Pin<Box<dyn Stream<Item = ViewerReport> + Send + 'static>>;

pub trait LiveViewerSource: Send + Sync {
    fn viewer_reports(&self) -> ViewerReportStream;
}

pub trait StreamOutputSignal: Send + 'static {
    fn is_active(&self) -> bool;
    fn changed(&mut self) -> impl Future<Output = Option<bool>> + Send;
}
