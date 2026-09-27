use std::time::Instant;

use forge_components::fmt_uptime;

pub struct RuntimeStatus {
    started_at: Instant,
    uptime_secs: u64,
}

impl RuntimeStatus {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            uptime_secs: 0,
        }
    }

    pub fn refresh(&mut self, now: Instant) {
        self.uptime_secs = now.saturating_duration_since(self.started_at).as_secs();
    }

    pub fn uptime_human(&self) -> String {
        fmt_uptime(self.uptime_secs)
    }
}
