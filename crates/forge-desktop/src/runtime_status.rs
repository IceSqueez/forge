/// Topic-scoped observable entity fed by the runtime→UI bridge. The bridge is the
/// sole owner of the bus→UI edge; it drains the runtime bus and applies each
/// relevant change here, then `cx.notify()`s so observing view-entities repaint.
/// Holds no runtime state of its own — only the values it has been handed.
pub struct RuntimeStatus {
    uptime_secs: u64,
}

impl RuntimeStatus {
    pub fn new() -> Self {
        Self { uptime_secs: 0 }
    }

    /// Advances uptime by one second per observed `timer.tick`. The bridge pairs
    /// this with `cx.notify()`; keeping the mutation free of `cx` leaves it
    /// directly exercisable.
    pub fn tick(&mut self) {
        self.uptime_secs = self.uptime_secs.saturating_add(1);
    }

    pub fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }
}
