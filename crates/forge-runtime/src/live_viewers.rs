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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    use forge_platform_core::ViewerReportStream;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
    use tokio_stream::wrappers::UnboundedReceiverStream;

    /// A `LiveViewerSource` whose report stream drains a channel the test drives.
    /// Holding the sender lets a test feed reports one at a time; dropping it ends
    /// the stream, exercising the slot-drop path.
    struct ChannelSource {
        rx: Mutex<Option<UnboundedReceiver<ViewerReport>>>,
    }

    fn channel_source() -> (Box<dyn LiveViewerSource>, UnboundedSender<ViewerReport>) {
        let (tx, rx) = unbounded_channel();
        let source = Box::new(ChannelSource {
            rx: Mutex::new(Some(rx)),
        });
        (source, tx)
    }

    impl LiveViewerSource for ChannelSource {
        fn viewer_reports(&self) -> ViewerReportStream {
            let rx = self
                .rx
                .lock()
                .expect("mutex poisoned")
                .take()
                .expect("viewer_reports called once");
            Box::pin(UnboundedReceiverStream::new(rx))
        }
    }

    /// Consume aggregate items until `expected` is observed. The watch channel
    /// coalesces (latest-value-wins), so intermediate values may be skipped; the
    /// bounded timeout turns a never-arriving value (e.g. a `Reporting(0)` a buggy
    /// impl collapsed to `Empty`) into a test failure instead of a hang.
    async fn settle_to<S>(stream: &mut S, expected: LiveViewerCount)
    where
        S: Stream<Item = LiveViewerCount> + Unpin,
    {
        let outcome = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(value) = stream.next().await {
                if value == expected {
                    return;
                }
            }
            panic!("aggregate stream ended before reaching {expected:?}");
        })
        .await;
        assert!(outcome.is_ok(), "timed out waiting for {expected:?}");
    }

    #[tokio::test]
    async fn sum_is_additive_across_registered_slots() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (src_a, tx_a) = channel_source();
        let (src_b, tx_b) = channel_source();
        handle.register(src_a);
        handle.register(src_b);

        tx_a.send(ViewerReport::Live { count: 3 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(3)).await;
        tx_b.send(ViewerReport::Live { count: 4 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(7)).await;
    }

    #[tokio::test]
    async fn absent_report_removes_only_its_own_slot() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (src_a, tx_a) = channel_source();
        let (src_b, tx_b) = channel_source();
        handle.register(src_a);
        handle.register(src_b);

        tx_a.send(ViewerReport::Live { count: 5 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(5)).await;
        tx_b.send(ViewerReport::Live { count: 2 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(7)).await;
        tx_a.send(ViewerReport::Absent).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(2)).await;
    }

    #[tokio::test]
    async fn absent_on_the_last_reporting_slot_yields_empty() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (src, tx) = channel_source();
        handle.register(src);

        tx.send(ViewerReport::Live { count: 5 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(5)).await;
        tx.send(ViewerReport::Absent).unwrap();
        settle_to(&mut sub, LiveViewerCount::Empty).await;
    }

    #[tokio::test]
    async fn stream_end_drops_only_that_slots_contribution() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (src_a, tx_a) = channel_source();
        let (src_b, tx_b) = channel_source();
        handle.register(src_a);
        handle.register(src_b);

        tx_a.send(ViewerReport::Live { count: 4 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(4)).await;
        tx_b.send(ViewerReport::Live { count: 6 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(10)).await;
        drop(tx_a);
        settle_to(&mut sub, LiveViewerCount::Reporting(6)).await;
    }

    #[tokio::test]
    async fn single_zero_report_is_reporting_zero_not_empty() {
        // Why: a platform reporting zero concurrent viewers (`Reporting(0)`) is a
        // distinct state from no platform reporting at all (`Empty`). Collapsing
        // one into the other is the highest-value regression this suite guards.
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (src, tx) = channel_source();
        handle.register(src);

        tx.send(ViewerReport::Live { count: 0 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(0)).await;
    }

    #[tokio::test]
    async fn late_subscriber_resynchronizes_to_current_value() {
        let handle = spawn_live_viewer_aggregator();
        let mut early = Box::pin(handle.subscribe());
        let (src, tx) = channel_source();
        handle.register(src);

        tx.send(ViewerReport::Live { count: 9 }).unwrap();
        settle_to(&mut early, LiveViewerCount::Reporting(9)).await;

        // A subscriber created only now must observe the current 9 on its first
        // poll, not the `Empty` the aggregate was seeded with at spawn.
        let mut late = Box::pin(handle.subscribe());
        assert_eq!(late.next().await, Some(LiveViewerCount::Reporting(9)));
    }
}
