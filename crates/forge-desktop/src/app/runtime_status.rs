use std::time::Instant;

use forge_components::fmt_uptime;

pub(crate) struct RuntimeStatus {
    started_at: Instant,
    uptime_secs: u64,
}

impl RuntimeStatus {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            uptime_secs: 0,
        }
    }

    pub(crate) fn refresh(&mut self, now: Instant) {
        self.uptime_secs = now.saturating_duration_since(self.started_at).as_secs();
    }

    pub(crate) fn uptime_human(&self) -> String {
        fmt_uptime(self.uptime_secs)
    }
}
