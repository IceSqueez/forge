use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_types::{MonotonicClock, RecoverMutex, SystemMonotonicClock};
use rhai::Dynamic;

#[derive(Clone)]
pub(crate) struct SandboxClock(Arc<dyn MonotonicClock + Send + Sync>);

impl SandboxClock {
    pub(crate) fn new(clock: Arc<dyn MonotonicClock + Send + Sync>) -> Self {
        Self(clock)
    }

    pub(crate) fn now(&self) -> Instant {
        self.0.now()
    }

    pub(crate) fn sleep(&self, duration: Duration) {
        self.0.sleep(duration);
    }
}

impl Default for SandboxClock {
    fn default() -> Self {
        Self::new(Arc::new(SystemMonotonicClock))
    }
}

impl fmt::Debug for SandboxClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SandboxClock")
    }
}

#[derive(Clone)]
pub(crate) struct WallBudget {
    clock: SandboxClock,
    limit: Duration,
    started_at: Arc<Mutex<Instant>>,
}

impl WallBudget {
    pub(crate) fn start(clock: SandboxClock, wall_time_ms: u64) -> Self {
        let started_at = Arc::new(Mutex::new(clock.now()));
        Self {
            clock,
            limit: Duration::from_millis(wall_time_ms),
            started_at,
        }
    }

    pub(crate) fn restart(&self) {
        *self.started_at.lock_recover() = self.clock.now();
    }

    pub(crate) fn elapsed(&self) -> Duration {
        let started_at = *self.started_at.lock_recover();
        self.clock.now().saturating_duration_since(started_at)
    }

    pub(crate) fn enforce_on(&self, engine: &mut rhai::Engine) {
        let budget = self.clone();
        engine.on_progress(move |_ops| (budget.elapsed() >= budget.limit).then_some(Dynamic::UNIT));
    }
}
