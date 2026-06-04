use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct DiscordBucket {
    pub(crate) limit: u32,
    pub(crate) remaining: u32,
    pub(crate) reset_after: Duration,
    pub(crate) updated_at: Instant,
}

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

    pub(crate) fn reset_hint_secs(&self) -> Option<f64> {
        if self.remaining > 0 || self.limit == 0 {
            return None;
        }
        let elapsed = self.updated_at.elapsed();
        let remaining = self.reset_after.saturating_sub(elapsed);
        if remaining > Duration::ZERO {
            Some(remaining.as_secs_f64())
        } else {
            None
        }
    }
}

pub(crate) struct DiscordRateLimiter {
    buckets: HashMap<String, DiscordBucket>,
    global_reset_at: Option<Instant>,
}

impl DiscordRateLimiter {
    pub(crate) fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            global_reset_at: None,
        }
    }

    pub(crate) fn acquire(&mut self, name: &str) -> RateLimitOutcome {
        self.buckets.entry(name.to_owned()).or_default().check()
    }

    pub(crate) fn record_response(
        &mut self,
        name: &str,
        limit: u32,
        remaining: u32,
        reset_after_secs: f64,
    ) {
        self.buckets
            .entry(name.to_owned())
            .or_default()
            .update_from_headers(limit, remaining, reset_after_secs);
    }

    pub(crate) fn observe_global_throttle(&mut self, duration: Duration) {
        self.global_reset_at = Some(Instant::now() + duration);
    }

    pub(crate) fn observe_remote_throttle(&mut self, name: &str, retry_after: Duration) {
        self.buckets
            .entry(name.to_owned())
            .or_default()
            .observe_remote_throttle(retry_after);
    }

    pub(crate) fn global_wait_duration(&self) -> Option<Duration> {
        let reset = self.global_reset_at?;
        let now = Instant::now();
        if reset > now { Some(reset - now) } else { None }
    }

    /// Returns `(remaining, total)` for the named webhook bucket.
    pub(crate) fn budget(&self, name: &str) -> (u64, u64) {
        self.buckets
            .get(name)
            .map_or((0, 0), |b| (b.remaining as u64, b.limit as u64))
    }

    /// Returns seconds until reset for the named bucket, when it is exhausted.
    pub(crate) fn reset_hint_secs(&self, name: &str) -> Option<f64> {
        self.buckets.get(name)?.reset_hint_secs()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn rate_limiter_acquire_uses_per_webhook_bucket() {
        let mut rl = DiscordRateLimiter::new();
        rl.record_response("alerts", 5, 2, 1.0);
        rl.record_response("clips", 5, 0, 1.0);

        assert!(matches!(rl.acquire("alerts"), RateLimitOutcome::Granted));
        assert!(matches!(
            rl.acquire("clips"),
            RateLimitOutcome::Throttled { .. }
        ));
    }

    #[test]
    fn rate_limiter_global_throttle_reports_wait() {
        let mut rl = DiscordRateLimiter::new();
        rl.observe_global_throttle(Duration::from_secs(30));
        let wait = rl.global_wait_duration();
        assert!(wait.is_some());
        let secs = wait.unwrap().as_secs_f64();
        assert!(secs > 0.0 && secs <= 30.0);
    }

    #[test]
    fn rate_limiter_global_throttle_clears_after_expiry() {
        let mut rl = DiscordRateLimiter::new();
        rl.observe_global_throttle(Duration::ZERO);
        assert!(rl.global_wait_duration().is_none());
    }

    #[test]
    fn rate_limiter_budget_returns_remaining_and_total() {
        let mut rl = DiscordRateLimiter::new();
        rl.record_response("alerts", 5, 3, 1.0);
        assert_eq!(rl.budget("alerts"), (3, 5));
        assert_eq!(rl.budget("unknown"), (0, 0));
    }

    #[test]
    fn rate_limiter_observe_remote_throttle_updates_bucket() {
        let mut rl = DiscordRateLimiter::new();
        rl.record_response("alerts", 5, 3, 1.0);
        rl.observe_remote_throttle("alerts", Duration::from_secs(2));
        assert_eq!(rl.budget("alerts").0, 0);
    }
}
