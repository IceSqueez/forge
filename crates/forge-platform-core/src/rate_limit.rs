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
#[allow(dead_code)]
fn _dyn_safe(_: &dyn RateLimiter) {}
