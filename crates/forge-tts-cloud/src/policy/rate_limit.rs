use std::time::Duration;

use async_trait::async_trait;
use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use tokio::sync::Mutex;
use tokio::time::Instant;

const EXHAUSTED_THRESHOLD: Duration = Duration::from_secs(60);

/// Floor-based rate limiter for synthesis calls.
///
/// Stores the earliest `Instant` at which the next call is permitted.
/// `observe_remote_throttle` advances the floor to `now + retry_after`.
/// `acquire` returns `Exhausted` when the floor is more than
/// `EXHAUSTED_THRESHOLD` in the future (server signalled a very long back-off).
pub struct SynthesisRateLimiter {
    next_allowed: Mutex<Option<Instant>>,
}

impl SynthesisRateLimiter {
    pub fn new() -> Self {
        Self {
            next_allowed: Mutex::new(None),
        }
    }
}

impl Default for SynthesisRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RateLimiter for SynthesisRateLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        let guard = self.next_allowed.lock().await;
        let floor = match *guard {
            None => return Ok(RateLimitOutcome::Granted),
            Some(f) => f,
        };
        let now = Instant::now();
        if now >= floor {
            return Ok(RateLimitOutcome::Granted);
        }
        let wait_for = floor - now;
        if wait_for > EXHAUSTED_THRESHOLD {
            Ok(RateLimitOutcome::Exhausted)
        } else {
            Ok(RateLimitOutcome::Throttled { wait_for })
        }
    }

    fn remaining(&self) -> u32 {
        match self.next_allowed.try_lock() {
            Ok(guard) => match *guard {
                None => 1,
                Some(f) => u32::from(Instant::now() >= f),
            },
            Err(_) => 1,
        }
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        let mut guard = self.next_allowed.lock().await;
        *guard = Some(Instant::now() + retry_after);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_limiter_grants_immediately() {
        let lim = SynthesisRateLimiter::new();
        let outcome = lim.acquire(1).await.unwrap();
        assert_eq!(outcome, RateLimitOutcome::Granted);
    }

    #[tokio::test]
    async fn throttled_after_observe() {
        tokio::time::pause();
        let lim = SynthesisRateLimiter::new();
        lim.observe_remote_throttle(Duration::from_secs(10)).await;
        let outcome = lim.acquire(1).await.unwrap();
        assert!(
            matches!(outcome, RateLimitOutcome::Throttled { wait_for } if wait_for <= Duration::from_secs(10))
        );
    }

    #[tokio::test]
    async fn granted_after_floor_passes() {
        tokio::time::pause();
        let lim = SynthesisRateLimiter::new();
        lim.observe_remote_throttle(Duration::from_secs(5)).await;
        tokio::time::advance(Duration::from_secs(6)).await;
        let outcome = lim.acquire(1).await.unwrap();
        assert_eq!(outcome, RateLimitOutcome::Granted);
    }

    #[tokio::test]
    async fn exhausted_when_floor_exceeds_threshold() {
        tokio::time::pause();
        let lim = SynthesisRateLimiter::new();
        lim.observe_remote_throttle(Duration::from_secs(90)).await;
        let outcome = lim.acquire(1).await.unwrap();
        assert_eq!(outcome, RateLimitOutcome::Exhausted);
    }

    #[tokio::test]
    async fn remaining_is_one_when_granted() {
        let lim = SynthesisRateLimiter::new();
        assert_eq!(lim.remaining(), 1);
    }

    #[tokio::test]
    async fn remaining_is_zero_when_throttled() {
        tokio::time::pause();
        let lim = SynthesisRateLimiter::new();
        lim.observe_remote_throttle(Duration::from_secs(10)).await;
        assert_eq!(lim.remaining(), 0);
    }
}
