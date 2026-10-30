use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use forge_platform_core::{AtomicConnectionState, ConnectionState};
use tokio::sync::watch;

use crate::health::HealthSnapshot;

#[derive(Debug, Clone)]
pub struct StreamOutputActive(watch::Receiver<bool>);

impl StreamOutputActive {
    pub fn is_active(&self) -> bool {
        *self.0.borrow()
    }

    pub fn into_receiver(self) -> watch::Receiver<bool> {
        self.0
    }

    pub async fn changed(&mut self) -> Option<bool> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ObsStreamStats {
    pub stream_active: bool,
    pub stream_duration: Option<Duration>,
    pub bitrate_kbps: Option<u32>,
    pub dropped_frames: u64,
    pub total_frames: u64,
    pub fps: f64,
    pub cpu_percent: f64,
}

#[derive(Debug, Clone)]
pub struct ObsStatsWatch(watch::Receiver<Option<ObsStreamStats>>);

impl ObsStatsWatch {
    pub fn current(&self) -> Option<ObsStreamStats> {
        *self.0.borrow()
    }

    pub async fn changed(&mut self) -> Option<Option<ObsStreamStats>> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}

pub(crate) struct StreamOutputChannels {
    active: watch::Sender<bool>,
    stats: watch::Sender<Option<ObsStreamStats>>,
}

pub(crate) type StreamOutputTarget = Arc<StreamOutputChannels>;

pub(crate) fn new_stream_output_target() -> StreamOutputTarget {
    Arc::new(StreamOutputChannels {
        active: watch::Sender::new(false),
        stats: watch::Sender::new(None),
    })
}

pub(crate) fn subscribe(target: &StreamOutputTarget) -> StreamOutputActive {
    StreamOutputActive(target.active.subscribe())
}

pub(crate) fn subscribe_stats(target: &StreamOutputTarget) -> ObsStatsWatch {
    ObsStatsWatch(target.stats.subscribe())
}

#[derive(Clone)]
pub(crate) struct StreamOutputFeed {
    target: Arc<Mutex<StreamOutputTarget>>,
    state: Arc<AtomicConnectionState>,
    health: Arc<RwLock<HealthSnapshot>>,
}

impl StreamOutputFeed {
    pub(crate) fn new(
        state: Arc<AtomicConnectionState>,
        health: Arc<RwLock<HealthSnapshot>>,
    ) -> Self {
        Self {
            target: Arc::new(Mutex::new(new_stream_output_target())),
            state,
            health,
        }
    }

    pub(crate) fn route_to(&self, target: StreamOutputTarget) {
        let mut guard = self.target.lock().unwrap_or_else(|e| e.into_inner());
        *guard = target;
        self.publish_locked(&guard);
    }

    pub(crate) fn detach(&self) {
        self.route_to(new_stream_output_target());
    }

    pub(crate) fn refresh(&self) {
        let guard = self.target.lock().unwrap_or_else(|e| e.into_inner());
        self.publish_locked(&guard);
    }

    fn publish_locked(&self, target: &StreamOutputTarget) {
        let connected = self.state.load() == ConnectionState::Connected;
        let stats = connected
            .then(|| self.health.read().ok().map(|health| health.stream_stats()))
            .flatten();
        let active = stats.is_some_and(|stats| stats.stream_active);
        target.active.send_if_modified(|current| {
            let changed = *current != active;
            *current = active;
            changed
        });
        target.stats.send_if_modified(|current| {
            let changed = *current != stats;
            *current = stats;
            changed
        });
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt as _;

    use super::*;

    fn live_snapshot() -> HealthSnapshot {
        HealthSnapshot {
            stream_active: true,
            bitrate_kbps: Some(6_000),
            ..HealthSnapshot::default()
        }
    }

    fn feed(state: ConnectionState) -> (StreamOutputFeed, StreamOutputTarget) {
        let feed = StreamOutputFeed::new(
            Arc::new(AtomicConnectionState::new(state)),
            Arc::new(RwLock::new(live_snapshot())),
        );
        let target = new_stream_output_target();
        feed.route_to(Arc::clone(&target));
        (feed, target)
    }

    #[test]
    fn a_connected_client_publishes_its_stats_and_stream_state() {
        let (_feed, target) = feed(ConnectionState::Connected);

        assert_eq!(
            (
                subscribe_stats(&target)
                    .current()
                    .and_then(|stats| stats.bitrate_kbps),
                subscribe(&target).is_active(),
            ),
            (Some(6_000), true)
        );
    }

    #[test]
    fn a_disconnected_client_publishes_no_stats_whatever_its_last_snapshot_said() {
        let (_feed, target) = feed(ConnectionState::Disconnected);

        assert_eq!(
            (
                subscribe_stats(&target).current(),
                subscribe(&target).is_active(),
            ),
            (None, false)
        );
    }

    #[test]
    fn losing_the_connection_withdraws_the_published_stats() {
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connected));
        let feed =
            StreamOutputFeed::new(Arc::clone(&state), Arc::new(RwLock::new(live_snapshot())));
        let target = new_stream_output_target();
        feed.route_to(Arc::clone(&target));

        state.store(ConnectionState::Disconnected);
        feed.refresh();

        assert_eq!(subscribe_stats(&target).current(), None);
    }

    #[test]
    fn a_refresh_with_the_same_stats_wakes_no_watcher() {
        let (feed, target) = feed(ConnectionState::Connected);
        let mut stats = subscribe_stats(&target);

        feed.refresh();

        assert!(stats.changed().now_or_never().is_none());
    }

    #[tokio::test]
    async fn the_stats_watch_carries_each_new_reading() {
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connected));
        let health = Arc::new(RwLock::new(live_snapshot()));
        let feed = StreamOutputFeed::new(state, Arc::clone(&health));
        let target = new_stream_output_target();
        feed.route_to(Arc::clone(&target));
        let mut stats = subscribe_stats(&target);

        health.write().unwrap().bitrate_kbps = Some(3_000);
        feed.refresh();

        let carried = stats.changed().await.flatten();
        assert_eq!(carried.and_then(|stats| stats.bitrate_kbps), Some(3_000));
    }

    #[tokio::test]
    async fn the_stats_watch_ends_when_its_target_is_gone() {
        let target = new_stream_output_target();
        let mut stats = subscribe_stats(&target);

        drop(target);

        assert_eq!(stats.changed().await, None);
    }
}
