use std::time::Instant;

/// Topic-scoped observable entity backing the footer uptime readout. The runtime
/// publishes no periodic tick, so uptime is measured against the moment this entity
/// was constructed: a boot-timestamp clock. A boot-started foreground loop recomputes
/// the elapsed seconds once per second and `cx.notify()`s so the footer repaints.
/// Holds no runtime state of its own — only the boot instant and the derived seconds.
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

    /// Recomputes uptime as the whole seconds elapsed since construction, saturating
    /// so a non-monotonic clock reading can never underflow (Windows monotonic epoch
    /// starts near zero). The clock loop passes `Instant::now()` and pairs this with
    /// `cx.notify()`; taking `now` as an argument keeps the mutation directly
    /// exercisable.
    pub fn refresh(&mut self, now: Instant) {
        self.uptime_secs = now.saturating_duration_since(self.started_at).as_secs();
    }

    /// Human-readable uptime for the footer: hours+minutes once past an hour,
    /// minutes+seconds once past a minute, else bare seconds (`"2h 14m"`,
    /// `"3m 7s"`, `"12s"`). Pure formatting, kept off `render` so it stays
    /// directly exercisable.
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
