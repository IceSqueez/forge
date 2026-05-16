use crate::PlatformError;
use async_trait::async_trait;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitOutcome {
    Granted,
    Throttled { wait_for: Duration },
    Exhausted,
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Attempts to consume `weight` tokens from the bucket.
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, PlatformError>;
    fn remaining(&self) -> u32;
    /// Adjusts the bucket using a `Retry-After` hint received from the platform (HTTP 429 / EventSub).
    async fn observe_remote_throttle(&self, retry_after: Duration);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes_are_distinct() {
        assert_ne!(RateLimitOutcome::Granted, RateLimitOutcome::Exhausted);
        assert_ne!(
            RateLimitOutcome::Granted,
            RateLimitOutcome::Throttled {
                wait_for: Duration::from_secs(1)
            },
        );
        assert_ne!(
            RateLimitOutcome::Exhausted,
            RateLimitOutcome::Throttled {
                wait_for: Duration::from_secs(1)
            },
        );
    }

    #[test]
    fn throttled_preserves_duration() {
        let d = Duration::from_millis(750);
        let outcome = RateLimitOutcome::Throttled { wait_for: d };
        assert!(matches!(outcome, RateLimitOutcome::Throttled { wait_for } if wait_for == d));
    }

    fn _dyn_safe(_: &dyn RateLimiter) {}
}
