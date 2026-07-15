use std::time::Instant;

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

    /// Saturating: a non-monotonic clock reading must not underflow (Windows monotonic epoch starts near zero).
    pub fn refresh(&mut self, now: Instant) {
        self.uptime_secs = now.saturating_duration_since(self.started_at).as_secs();
    }

    pub fn uptime_human(&self) -> String {
        let secs = self.uptime_secs;
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        if hours > 0 {
            format!("{hours}h {minutes}m")
        } else if minutes > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{seconds}s")
        }
    }
}
