use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use forge_platform_core::{LiveViewerSource, ViewerReport};
use forge_types::PlatformId;
use futures_core::Stream;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::task::AbortHandle;
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
    Attach {
        generation: u64,
        platform: PlatformId,
    },
    Report {
        generation: u64,
        platform: PlatformId,
        report: ViewerReport,
    },
    Detach {
        generation: u64,
        platform: PlatformId,
    },
}

#[derive(Clone)]
pub struct LiveViewerAggregatorHandle {
    commands: mpsc::Sender<AggregatorCommand>,
    output: watch::Receiver<LiveViewerCount>,
    live_platforms: watch::Receiver<LivePlatforms>,
    next_generation: Arc<AtomicU64>,
    forwarders: Arc<Mutex<BTreeMap<PlatformId, AbortHandle>>>,
    runtime: Handle,
}

impl LiveViewerAggregatorHandle {
    pub fn register(&self, platform: PlatformId, source: Box<dyn LiveViewerSource>) {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let commands = self.commands.clone();
        let forwarder = self.runtime.spawn(async move {
            if commands
                .send(AggregatorCommand::Attach {
                    generation,
                    platform,
                })
                .await
                .is_err()
            {
                return;
            }
            let mut stream = source.viewer_reports();
            while let Some(report) = stream.next().await {
                if commands
                    .send(AggregatorCommand::Report {
                        generation,
                        platform,
                        report,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = commands
                .send(AggregatorCommand::Detach {
                    generation,
                    platform,
                })
                .await;
        });
        let replaced = self
            .lock_forwarders()
            .insert(platform, forwarder.abort_handle());
        if let Some(previous) = replaced {
            previous.abort();
        }
    }

    pub fn unregister(&self, platform: PlatformId) {
        if let Some(previous) = self.lock_forwarders().remove(&platform) {
            previous.abort();
        }
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let commands = self.commands.clone();
        self.runtime.spawn(async move {
            let _ = commands
                .send(AggregatorCommand::Detach {
                    generation,
                    platform,
                })
                .await;
        });
    }

    pub fn subscribe(&self) -> impl Stream<Item = LiveViewerCount> + Send + 'static {
        WatchStream::new(self.output.clone())
    }

    pub(crate) fn live_platforms(&self) -> watch::Receiver<LivePlatforms> {
        self.live_platforms.clone()
    }

    fn lock_forwarders(&self) -> std::sync::MutexGuard<'_, BTreeMap<PlatformId, AbortHandle>> {
        self.forwarders.lock().unwrap_or_else(|e| e.into_inner())
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
        next_generation: Arc::new(AtomicU64::new(0)),
        forwarders: Arc::new(Mutex::new(BTreeMap::new())),
        runtime: Handle::current(),
    }
}

#[derive(Default)]
struct PlatformSlots {
    generations: BTreeMap<PlatformId, u64>,
    counts: BTreeMap<PlatformId, u64>,
}

impl PlatformSlots {
    fn claim(&mut self, platform: PlatformId, generation: u64) -> bool {
        match self.generations.get(&platform) {
            Some(current) if generation < *current => false,
            _ => {
                self.generations.insert(platform, generation);
                true
            }
        }
    }

    fn apply(&mut self, command: AggregatorCommand) {
        match command {
            AggregatorCommand::Report {
                generation,
                platform,
                report: ViewerReport::Live { count },
            } => {
                if self.claim(platform, generation) {
                    self.counts.insert(platform, count);
                }
            }
            AggregatorCommand::Attach {
                generation,
                platform,
            }
            | AggregatorCommand::Detach {
                generation,
                platform,
            }
            | AggregatorCommand::Report {
                generation,
                platform,
                report: ViewerReport::Absent,
            } => {
                if self.claim(platform, generation) {
                    self.counts.remove(&platform);
                }
            }
        }
    }

    fn total(&self) -> LiveViewerCount {
        if self.counts.is_empty() {
            LiveViewerCount::Empty
        } else {
            LiveViewerCount::Reporting(self.counts.values().copied().fold(0, u64::saturating_add))
        }
    }

    fn live_platforms(&self) -> LivePlatforms {
        self.counts.keys().copied().collect()
    }
}

async fn aggregate(
    mut commands: mpsc::Receiver<AggregatorCommand>,
    output: watch::Sender<LiveViewerCount>,
    live_platforms: watch::Sender<LivePlatforms>,
) {
    let mut slots = PlatformSlots::default();
    while let Some(command) = commands.recv().await {
        slots.apply(command);
        let next = slots.total();
        output.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
        let next_platforms = slots.live_platforms();
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
    use std::time::Duration;

    use crate::test_support::channel_viewer_source as channel_source;

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
        handle.register(PlatformId::Twitch, src_a);
        handle.register(PlatformId::YouTube, src_b);

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
        handle.register(PlatformId::Twitch, src_a);
        handle.register(PlatformId::YouTube, src_b);

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
        handle.register(PlatformId::Kick, src);

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
        handle.register(PlatformId::Twitch, src_a);
        handle.register(PlatformId::YouTube, src_b);

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
        handle.register(PlatformId::Kick, src);

        tx.send(ViewerReport::Live { count: 0 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(0)).await;
    }

    #[tokio::test]
    async fn late_subscriber_resynchronizes_to_current_value() {
        let handle = spawn_live_viewer_aggregator();
        let mut early = Box::pin(handle.subscribe());
        let (src, tx) = channel_source();
        handle.register(PlatformId::Kick, src);

        tx.send(ViewerReport::Live { count: 9 }).unwrap();
        settle_to(&mut early, LiveViewerCount::Reporting(9)).await;

        let mut late = Box::pin(handle.subscribe());
        assert_eq!(late.next().await, Some(LiveViewerCount::Reporting(9)));
    }
}
