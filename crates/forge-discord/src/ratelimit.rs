use std::time::{Duration, Instant};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DiscordBucket {
    pub(crate) limit: u32,
    pub(crate) remaining: u32,
    pub(crate) reset_after: Duration,
    pub(crate) updated_at: Instant,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum RateLimitOutcome {
    Granted,
    Throttled { wait_for: Duration },
}

impl Default for DiscordBucket {
    fn default() -> Self {
        Self {
            limit: 0,
            remaining: 0,
            reset_after: Duration::ZERO,
            updated_at: Instant::now(),
        }
    }
}

#[allow(dead_code)]
impl DiscordBucket {
    pub(crate) fn check(&mut self) -> RateLimitOutcome {
        if self.limit == 0 {
            return RateLimitOutcome::Granted;
        }
        let elapsed = self.updated_at.elapsed();
        if elapsed >= self.reset_after {
            self.remaining = self.limit;
        }
        if self.remaining == 0 {
            let wait_for = self.reset_after.saturating_sub(elapsed);
            return RateLimitOutcome::Throttled { wait_for };
        }
        self.remaining = self.remaining.saturating_sub(1);
        RateLimitOutcome::Granted
    }

    pub(crate) fn update_from_headers(
        &mut self,
        limit: u32,
        remaining: u32,
        reset_after_secs: f64,
    ) {
        self.limit = limit;
        self.remaining = remaining;
        self.reset_after = Duration::from_secs_f64(reset_after_secs);
        self.updated_at = Instant::now();
    }

    pub(crate) fn observe_remote_throttle(&mut self, retry_after: Duration) {
        self.remaining = 0;
        self.reset_after = retry_after;
        self.updated_at = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_grants_when_remaining() {
        let mut b = DiscordBucket::default();
        b.update_from_headers(5, 4, 1.0);
        assert!(matches!(b.check(), RateLimitOutcome::Granted));
        assert_eq!(b.remaining, 3);
    }

    #[test]
    fn bucket_throttles_when_exhausted() {
        let mut b = DiscordBucket::default();
        b.update_from_headers(5, 0, 2.0);
        assert!(matches!(b.check(), RateLimitOutcome::Throttled { .. }));
    }

    #[test]
    fn observe_remote_throttle_exhausts_bucket() {
        let mut b = DiscordBucket::default();
        b.update_from_headers(5, 3, 1.0);
        b.observe_remote_throttle(Duration::from_secs(2));
        assert_eq!(b.remaining, 0);
    }

    #[test]
    fn uninitialised_bucket_always_grants() {
        let mut b = DiscordBucket::default();
        assert!(matches!(b.check(), RateLimitOutcome::Granted));
    }
}
