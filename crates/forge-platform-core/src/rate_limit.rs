use crate::PlatformError;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

/// Leaky-token-bucket limiter shared across every API path that draws on a
/// single per-credential budget (e.g. one Twitch client-id has one Helix
/// budget regardless of how many transports issue requests).
///
/// Tokens refill continuously at `capacity / refill_per`. The whole-token
/// arithmetic and the cooldown deadline live behind a `std::sync::Mutex`
/// because the critical section is fully synchronous — there is no `.await`
/// between locking and dropping the guard, so a tokio mutex would only add
/// overhead and an await-holding-lock hazard.
pub struct TokenBucketRateLimiter {
    capacity: u32,
    refill_per: Duration,
    state: Mutex<BucketState>,
}

struct BucketState {
    tokens: f64,
    last_refill: Instant,
    /// While `now < cooldown_until`, acquire always throttles for the remaining
    /// span. Set from a platform `Retry-After`, so we honour the server's own
    /// back-off rather than only our local estimate.
    cooldown_until: Option<Instant>,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u32, refill_per: Duration) -> Self {
        Self {
            capacity,
            refill_per,
            state: Mutex::new(BucketState {
                tokens: capacity as f64,
                last_refill: Instant::now(),
                cooldown_until: None,
            }),
        }
    }

    /// Tokens regenerated per second; zero refill window means no regeneration.
    fn tokens_per_sec(&self) -> f64 {
        let secs = self.refill_per.as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            self.capacity as f64 / secs
        }
    }

    /// Adds tokens for the time elapsed since `last_refill`, capped at capacity.
    /// `saturating_duration_since` avoids the Windows monotonic-clock panic that
    /// `now - last_refill` triggers when the process starts near the clock epoch.
    fn refill(&self, state: &mut BucketState, now: Instant) {
        let elapsed = now.saturating_duration_since(state.last_refill);
        state.last_refill = now;
        let gained = elapsed.as_secs_f64() * self.tokens_per_sec();
        state.tokens = (state.tokens + gained).min(self.capacity as f64);
    }

    /// Seconds until the bucket holds `weight` whole tokens at the refill rate.
    fn wait_for_tokens(&self, current: f64, weight: u32) -> Duration {
        let deficit = weight as f64 - current;
        let rate = self.tokens_per_sec();
        if deficit <= 0.0 || rate <= 0.0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(deficit / rate)
    }
}

#[async_trait]
impl RateLimiter for TokenBucketRateLimiter {
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        // A request costing more than the whole budget can never be satisfied.
        if weight > self.capacity {
            return Ok(RateLimitOutcome::Exhausted);
        }
        let now = Instant::now();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(deadline) = state.cooldown_until {
            let remaining = deadline.saturating_duration_since(now);
            if remaining > Duration::ZERO {
                return Ok(RateLimitOutcome::Throttled {
                    wait_for: remaining,
                });
            }
            state.cooldown_until = None;
        }

        self.refill(&mut state, now);
        if state.tokens >= weight as f64 {
            state.tokens -= weight as f64;
            Ok(RateLimitOutcome::Granted)
        } else {
            let wait_for = self.wait_for_tokens(state.tokens, weight);
            Ok(RateLimitOutcome::Throttled { wait_for })
        }
    }

    fn remaining(&self) -> u32 {
        let now = Instant::now();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Approximate the refill without mutating shared state; callers treat
        // `remaining` as a hint, not a reservation.
        let elapsed = now.saturating_duration_since(state.last_refill);
        let gained = elapsed.as_secs_f64() * self.tokens_per_sec();
        (state.tokens + gained).min(self.capacity as f64) as u32
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        let now = Instant::now();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Honour the server's back-off and drain the local bucket so we do not
        // immediately re-fire once the cooldown lifts.
        state.cooldown_until = Some(now + retry_after);
        state.tokens = 0.0;
        state.last_refill = now;
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn _dyn_safe(_: &dyn RateLimiter) {}
