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
        let outcome = tokio::time::timeout(Duration::from_secs(30), async {
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

    async fn settle_platforms(handle: &LiveViewerAggregatorHandle, expected: &[PlatformId]) {
        let expected: LivePlatforms = expected.iter().copied().collect();
        let mut platforms = handle.live_platforms();
        let outcome = tokio::time::timeout(
            Duration::from_secs(30),
            platforms.wait_for(|current| *current == expected),
        )
        .await;
        assert!(
            matches!(outcome, Ok(Ok(_))),
            "live platforms never became {expected:?}"
        );
    }

    fn attach(platform: PlatformId, generation: u64) -> AggregatorCommand {
        AggregatorCommand::Attach {
            generation,
            platform,
        }
    }

    fn detach(platform: PlatformId, generation: u64) -> AggregatorCommand {
        AggregatorCommand::Detach {
            generation,
            platform,
        }
    }

    fn live(platform: PlatformId, generation: u64, count: u64) -> AggregatorCommand {
        AggregatorCommand::Report {
            generation,
            platform,
            report: ViewerReport::Live { count },
        }
    }

    fn absent(platform: PlatformId, generation: u64) -> AggregatorCommand {
        AggregatorCommand::Report {
            generation,
            platform,
            report: ViewerReport::Absent,
        }
    }

    fn replay(commands: Vec<AggregatorCommand>) -> PlatformSlots {
        let mut slots = PlatformSlots::default();
        for command in commands {
            slots.apply(command);
        }
        slots
    }

    #[test]
    fn commands_from_a_superseded_generation_never_touch_the_current_slot() {
        let cases: Vec<(&str, Vec<AggregatorCommand>, LiveViewerCount)> = vec![
            (
                "stale live report after the replacement attached",
                vec![
                    attach(PlatformId::Twitch, 1),
                    live(PlatformId::Twitch, 1, 5),
                    attach(PlatformId::Twitch, 2),
                    live(PlatformId::Twitch, 2, 3),
                    live(PlatformId::Twitch, 1, 9),
                ],
                LiveViewerCount::Reporting(3),
            ),
            (
                "stale absent report after the replacement reported",
                vec![
                    attach(PlatformId::Twitch, 1),
                    attach(PlatformId::Twitch, 2),
                    live(PlatformId::Twitch, 2, 4),
                    absent(PlatformId::Twitch, 1),
                ],
                LiveViewerCount::Reporting(4),
            ),
            (
                "stale detach from the replaced forwarder ending late",
                vec![
                    attach(PlatformId::Twitch, 1),
                    attach(PlatformId::Twitch, 2),
                    live(PlatformId::Twitch, 2, 4),
                    detach(PlatformId::Twitch, 1),
                ],
                LiveViewerCount::Reporting(4),
            ),
            (
                "queued report from the source an unregister removed",
                vec![
                    attach(PlatformId::Twitch, 1),
                    live(PlatformId::Twitch, 1, 6),
                    detach(PlatformId::Twitch, 2),
                    live(PlatformId::Twitch, 1, 6),
                ],
                LiveViewerCount::Empty,
            ),
        ];
        for (case, commands, expected) in cases {
            assert_eq!(replay(commands).total(), expected, "{case}");
        }
    }

    #[test]
    fn attaching_a_replacement_clears_the_previous_sources_count() {
        let slots = replay(vec![
            attach(PlatformId::Kick, 1),
            live(PlatformId::Kick, 1, 8),
            attach(PlatformId::Kick, 2),
        ]);
        assert_eq!(slots.total(), LiveViewerCount::Empty);
    }

    #[test]
    fn a_newer_generation_on_one_platform_does_not_lock_out_older_generations_elsewhere() {
        let slots = replay(vec![
            attach(PlatformId::Twitch, 5),
            live(PlatformId::Twitch, 5, 1),
            live(PlatformId::YouTube, 2, 10),
        ]);
        assert_eq!(slots.total(), LiveViewerCount::Reporting(11));
    }

    #[test]
    fn live_platforms_list_every_platform_with_a_live_report_including_zero_viewers() {
        let slots = replay(vec![
            live(PlatformId::Twitch, 1, 0),
            live(PlatformId::Kick, 2, 4),
            live(PlatformId::YouTube, 3, 2),
            absent(PlatformId::YouTube, 3),
        ]);
        assert_eq!(
            slots.live_platforms(),
            [PlatformId::Twitch, PlatformId::Kick].into_iter().collect()
        );
    }

    #[tokio::test]
    async fn registering_again_for_a_platform_replaces_its_previous_source() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (first, first_tx) = channel_source();
        let (other, other_tx) = channel_source();
        handle.register(PlatformId::Twitch, first);
        handle.register(PlatformId::YouTube, other);
        first_tx.send(ViewerReport::Live { count: 5 }).unwrap();
        other_tx.send(ViewerReport::Live { count: 1 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(6)).await;

        let (second, second_tx) = channel_source();
        handle.register(PlatformId::Twitch, second);
        second_tx.send(ViewerReport::Live { count: 2 }).unwrap();

        settle_to(&mut sub, LiveViewerCount::Reporting(3)).await;
    }

    #[tokio::test]
    async fn replaced_source_stops_being_read() {
        let handle = spawn_live_viewer_aggregator();
        let (first, first_tx) = channel_source();
        handle.register(PlatformId::Twitch, first);
        let (second, _second_tx) = channel_source();
        handle.register(PlatformId::Twitch, second);

        let outcome = tokio::time::timeout(Duration::from_secs(30), first_tx.closed()).await;

        assert!(outcome.is_ok(), "replaced source was still being polled");
    }

    #[tokio::test]
    async fn unregister_drops_the_platform_viewers() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (twitch, twitch_tx) = channel_source();
        let (youtube, youtube_tx) = channel_source();
        handle.register(PlatformId::Twitch, twitch);
        handle.register(PlatformId::YouTube, youtube);
        twitch_tx.send(ViewerReport::Live { count: 5 }).unwrap();
        youtube_tx.send(ViewerReport::Live { count: 2 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(7)).await;

        handle.unregister(PlatformId::Twitch);

        settle_to(&mut sub, LiveViewerCount::Reporting(2)).await;
    }

    #[tokio::test]
    async fn unregister_drops_the_platform_from_the_live_set() {
        let handle = spawn_live_viewer_aggregator();
        let (twitch, twitch_tx) = channel_source();
        let (kick, kick_tx) = channel_source();
        handle.register(PlatformId::Twitch, twitch);
        handle.register(PlatformId::Kick, kick);
        twitch_tx.send(ViewerReport::Live { count: 0 }).unwrap();
        kick_tx.send(ViewerReport::Live { count: 3 }).unwrap();
        settle_platforms(&handle, &[PlatformId::Twitch, PlatformId::Kick]).await;

        handle.unregister(PlatformId::Twitch);

        settle_platforms(&handle, &[PlatformId::Kick]).await;
    }

    #[tokio::test]
    async fn platform_reports_again_after_unregister_and_fresh_register() {
        let handle = spawn_live_viewer_aggregator();
        let mut sub = Box::pin(handle.subscribe());
        let (first, first_tx) = channel_source();
        handle.register(PlatformId::Kick, first);
        first_tx.send(ViewerReport::Live { count: 5 }).unwrap();
        settle_to(&mut sub, LiveViewerCount::Reporting(5)).await;
        handle.unregister(PlatformId::Kick);
        settle_to(&mut sub, LiveViewerCount::Empty).await;

        let (second, second_tx) = channel_source();
        handle.register(PlatformId::Kick, second);
        second_tx.send(ViewerReport::Live { count: 4 }).unwrap();

        settle_to(&mut sub, LiveViewerCount::Reporting(4)).await;
    }
}
