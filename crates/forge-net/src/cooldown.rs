use std::time::{Duration, Instant};

const SECS_PER_HOUR: u64 = 60 * 60;

pub const MAX_REMOTE_COOLDOWN: Duration = Duration::from_secs(SECS_PER_HOUR);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cooldown {
    until: Option<Instant>,
}

impl Cooldown {
    pub fn extend(&mut self, now: Instant, retry_after: Duration) {
        let Some(candidate) = now.checked_add(retry_after.min(MAX_REMOTE_COOLDOWN)) else {
            return;
        };
        if self.until.is_none_or(|current| candidate > current) {
            self.until = Some(candidate);
        }
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.until
            .map_or(Duration::ZERO, |until| until.saturating_duration_since(now))
    }
}
