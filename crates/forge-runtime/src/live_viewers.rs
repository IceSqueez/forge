use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use forge_platform_core::{LiveViewerSource, ViewerReport};
use futures_core::Stream;
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::WatchStream;

const COMMAND_CHANNEL_CAP: usize = 64;

/// The summed concurrent-viewer figure across connected reporting platforms.
/// `Empty` is distinct from `Reporting(0)`: `Empty` means no connected platform
/// currently reports a figure, while `Reporting(0)` means one or more report and
/// their figures sum to zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveViewerCount {
    Reporting(u64),
    Empty,
}

enum AggregatorCommand {
    Report { slot: u64, report: ViewerReport },
    Drop { slot: u64 },
}

#[derive(Clone)]
pub struct LiveViewerAggregatorHandle {
    commands: mpsc::Sender<AggregatorCommand>,
    output: watch::Receiver<LiveViewerCount>,
    next_slot: Arc<AtomicU64>,
}

impl LiveViewerAggregatorHandle {
    /// Wires a platform's viewer-report capability into the aggregate. Each call
    /// occupies a distinct additive slot (no cross-platform dedup); the slot's
    /// contribution drops on `Absent` or when the source's stream ends.
    pub fn register(&self, source: Box<dyn LiveViewerSource>) {
        let slot = self.next_slot.fetch_add(1, Ordering::Relaxed);
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let mut stream = source.viewer_reports();
            while let Some(report) = stream.next().await {
                if commands
                    .send(AggregatorCommand::Report { slot, report })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = commands.send(AggregatorCommand::Drop { slot }).await;
        });
    }

    /// Yields the current aggregate immediately on first poll, then on every
    /// change. Latest-value-wins: a slow consumer resynchronizes to the newest
    /// figure and never blocks the aggregator.
    pub fn subscribe(&self) -> impl Stream<Item = LiveViewerCount> + Send + 'static {
        WatchStream::new(self.output.clone())
    }
}

pub fn spawn_live_viewer_aggregator() -> LiveViewerAggregatorHandle {
    let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_CAP);
    let (output_tx, output_rx) = watch::channel(LiveViewerCount::Empty);
    tokio::spawn(aggregate(command_rx, output_tx));
    LiveViewerAggregatorHandle {
        commands: command_tx,
        output: output_rx,
        next_slot: Arc::new(AtomicU64::new(0)),
    }
}

async fn aggregate(
    mut commands: mpsc::Receiver<AggregatorCommand>,
    output: watch::Sender<LiveViewerCount>,
) {
    let mut reporting: BTreeMap<u64, u64> = BTreeMap::new();
    while let Some(command) = commands.recv().await {
        match command {
            AggregatorCommand::Report {
                slot,
                report: ViewerReport::Live { count },
            } => {
                reporting.insert(slot, count);
            }
            AggregatorCommand::Report {
                slot,
                report: ViewerReport::Absent,
            }
            | AggregatorCommand::Drop { slot } => {
                reporting.remove(&slot);
            }
        }
        let next = if reporting.is_empty() {
            LiveViewerCount::Empty
        } else {
            LiveViewerCount::Reporting(reporting.values().copied().fold(0, u64::saturating_add))
        };
        output.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
    }
}
