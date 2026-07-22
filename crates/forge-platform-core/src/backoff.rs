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

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_ceiling_ms(base_ms: u64, cap_ms: u64, attempt: u32) -> u64 {
        base_ms
            .saturating_mul(1u64.checked_shl(attempt.min(32)).unwrap_or(u64::MAX))
            .min(cap_ms)
    }

    #[test]
    fn next_delay_stays_within_full_jitter_ceiling_up_to_and_past_crossover() {
        let base = Duration::from_millis(100);
        let cap = Duration::from_millis(3200);
        for _ in 0..64 {
            let mut backoff = Backoff::new(base, cap);
            for attempt in 0..12u32 {
                let ceiling = spec_ceiling_ms(100, 3200, attempt);
                let delay_ms = backoff.next_delay().as_millis() as u64;
                assert!(
                    delay_ms <= ceiling,
                    "attempt {attempt}: {delay_ms}ms exceeded ceiling {ceiling}ms"
                );
            }
        }
    }

    #[test]
    fn next_delay_saturates_at_cap_past_shift_limit_without_overflow() {
        let base = Duration::from_millis(5_000_000_000);
        let cap = Duration::from_millis(60_000);
        let mut backoff = Backoff::new(base, cap);
        for attempt in 0..40u32 {
            let delay_ms = backoff.next_delay().as_millis() as u64;
            assert!(
                delay_ms <= 60_000,
                "attempt {attempt}: {delay_ms}ms exceeded cap 60000ms"
            );
        }
    }

    #[test]
    fn reset_returns_ceiling_from_cap_back_to_base() {
        let base = Duration::from_millis(100);
        let cap = Duration::from_millis(60_000);
        let mut backoff = Backoff::new(base, cap);
        for _ in 0..12 {
            backoff.next_delay();
        }
        for _ in 0..256 {
            backoff.reset();
            let delay_ms = backoff.next_delay().as_millis() as u64;
            assert!(
                delay_ms <= 100,
                "after reset delay {delay_ms}ms exceeded base ceiling 100ms"
            );
        }
    }

    #[test]
    fn attempt_counter_increments_per_delay_and_zeroes_on_reset() {
        let mut backoff = Backoff::new(Duration::from_millis(10), Duration::from_millis(100));
        assert_eq!(backoff.attempt(), 0);
        for expected in 1..=5u32 {
            backoff.next_delay();
            assert_eq!(backoff.attempt(), expected);
        }
        backoff.reset();
        assert_eq!(backoff.attempt(), 0);
    }

    #[test]
    fn with_cap_draws_from_canonical_base_at_first_attempt() {
        for _ in 0..256 {
            let mut backoff = Backoff::with_cap(Duration::from_millis(10_000));
            let delay_ms = backoff.next_delay().as_millis() as u64;
            assert!(
                delay_ms <= 250,
                "first delay {delay_ms}ms exceeded canonical base ceiling 250ms"
            );
        }
    }

    #[test]
    fn default_curve_starts_at_250ms_base_and_never_exceeds_60s_cap() {
        for _ in 0..64 {
            let mut backoff = Backoff::default();
            assert!(
                backoff.next_delay().as_millis() as u64 <= 250,
                "default first delay exceeded 250ms base"
            );
            for _ in 0..39 {
                assert!(
                    backoff.next_delay().as_millis() as u64 <= 60_000,
                    "default delay exceeded 60s cap"
                );
            }
        }
    }
}
