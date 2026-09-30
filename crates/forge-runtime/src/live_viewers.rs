use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use forge_platform_core::{LiveViewerSource, ViewerReport};
use forge_types::PlatformId;
use futures_core::Stream;
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::WatchStream;

const COMMAND_CHANNEL_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveViewerCount {
    Reporting(u64),
    Empty,
}

pub(crate) type LivePlatforms = BTreeSet<PlatformId>;

enum AggregatorCommand {
    Report {
        slot: u64,
        platform: PlatformId,
        report: ViewerReport,
    },
    Drop {
        slot: u64,
    },
}

#[derive(Clone)]
pub struct LiveViewerAggregatorHandle {
    commands: mpsc::Sender<AggregatorCommand>,
    output: watch::Receiver<LiveViewerCount>,
    live_platforms: watch::Receiver<LivePlatforms>,
    next_slot: Arc<AtomicU64>,
}

impl LiveViewerAggregatorHandle {
    pub fn register(&self, platform: PlatformId, source: Box<dyn LiveViewerSource>) {
        let slot = self.next_slot.fetch_add(1, Ordering::Relaxed);
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let mut stream = source.viewer_reports();
            while let Some(report) = stream.next().await {
                if commands
                    .send(AggregatorCommand::Report {
                        slot,
                        platform,
                        report,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = commands.send(AggregatorCommand::Drop { slot }).await;
        });
    }

    pub fn subscribe(&self) -> impl Stream<Item = LiveViewerCount> + Send + 'static {
        WatchStream::new(self.output.clone())
    }

    pub(crate) fn live_platforms(&self) -> watch::Receiver<LivePlatforms> {
        self.live_platforms.clone()
    }
}

pub fn spawn_live_viewer_aggregator() -> LiveViewerAggregatorHandle {
    let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_CAP);
    let (output_tx, output_rx) = watch::channel(LiveViewerCount::Empty);
    let (platforms_tx, platforms_rx) = watch::channel(LivePlatforms::new());
    tokio::spawn(aggregate(command_rx, output_tx, platforms_tx));
    LiveViewerAggregatorHandle {
        commands: command_tx,
        output: output_rx,
        live_platforms: platforms_rx,
        next_slot: Arc::new(AtomicU64::new(0)),
    }
}

async fn aggregate(
    mut commands: mpsc::Receiver<AggregatorCommand>,
    output: watch::Sender<LiveViewerCount>,
    live_platforms: watch::Sender<LivePlatforms>,
) {
    let mut reporting: BTreeMap<u64, (PlatformId, u64)> = BTreeMap::new();
    while let Some(command) = commands.recv().await {
        match command {
            AggregatorCommand::Report {
                slot,
                platform,
                report: ViewerReport::Live { count },
            } => {
                reporting.insert(slot, (platform, count));
            }
            AggregatorCommand::Report {
                slot,
                report: ViewerReport::Absent,
                ..
            }
            | AggregatorCommand::Drop { slot } => {
                reporting.remove(&slot);
            }
        }
        let next = if reporting.is_empty() {
            LiveViewerCount::Empty
        } else {
            LiveViewerCount::Reporting(
                reporting
                    .values()
                    .map(|(_, count)| *count)
                    .fold(0, u64::saturating_add),
            )
        };
        output.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
        let next_platforms: LivePlatforms =
            reporting.values().map(|(platform, _)| *platform).collect();
        live_platforms.send_if_modified(|current| {
            if *current == next_platforms {
                false
            } else {
                *current = next_platforms;
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

        let mut late = Box::pin(handle.subscribe());
        assert_eq!(late.next().await, Some(LiveViewerCount::Reporting(9)));
    }
}
