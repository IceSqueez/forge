use std::time::Duration;

use rand::RngExt;

const DEFAULT_BASE: Duration = Duration::from_millis(250);
const DEFAULT_CAP: Duration = Duration::from_secs(60);

pub struct Backoff {
    base_ms: u64,
    cap_ms: u64,
    attempt: u32,
}

impl Backoff {
    pub fn new(base: Duration, cap: Duration) -> Self {
        Self {
            base_ms: base.as_millis().min(u128::from(u64::MAX)) as u64,
            cap_ms: cap.as_millis().min(u128::from(u64::MAX)) as u64,
            attempt: 0,
        }
    }

    pub fn with_cap(cap: Duration) -> Self {
        Self::new(DEFAULT_BASE, cap)
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    pub fn next_delay(&mut self) -> Duration {
        let ceiling = self
            .base_ms
            .saturating_mul(1u64.checked_shl(self.attempt.min(32)).unwrap_or(u64::MAX))
            .min(self.cap_ms);
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_millis(rand::rng().random_range(0..=ceiling))
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(DEFAULT_BASE, DEFAULT_CAP)
    }
}
