use std::sync::{Arc, Mutex, RwLock};

use forge_platform_core::{AtomicConnectionState, ConnectionState};
use tokio::sync::watch;

use crate::health::HealthSnapshot;

/// Reads `false` whenever OBS is not connected, whatever the last reported output state was.
#[derive(Debug, Clone)]
pub struct StreamOutputActive(watch::Receiver<bool>);

impl StreamOutputActive {
    pub fn is_active(&self) -> bool {
        *self.0.borrow()
    }

    /// Coalesces flips since the last wake into the latest value; `None` once the publishing side is dropped.
    pub async fn changed(&mut self) -> Option<bool> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}

pub(crate) type StreamOutputTarget = Arc<watch::Sender<bool>>;

pub(crate) fn new_stream_output_target() -> StreamOutputTarget {
    Arc::new(watch::Sender::new(false))
}

pub(crate) fn subscribe(target: &StreamOutputTarget) -> StreamOutputActive {
    StreamOutputActive(target.subscribe())
}

#[derive(Clone)]
pub(crate) struct StreamOutputFeed {
    /// Held across reading the inputs and sending, so a publish computed from stale inputs cannot land after a newer one.
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
        let active = self.state.load() == ConnectionState::Connected
            && self
                .health
                .read()
                .is_ok_and(|snapshot| snapshot.stream_active);
        target.send_if_modified(|current| {
            let changed = *current != active;
            *current = active;
            changed
        });
    }
}
