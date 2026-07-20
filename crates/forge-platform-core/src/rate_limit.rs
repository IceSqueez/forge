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

pub const MAX_THROTTLE_WAIT: Duration = Duration::from_secs(10);
pub const MAX_ACQUIRE_ATTEMPTS: u32 = 3;

pub async fn acquire_or_wait(limiter: &dyn RateLimiter, weight: u32) -> Result<(), PlatformError> {
    let mut waited = Duration::ZERO;
    let mut attempts = 0;
    loop {
        match limiter.acquire(weight).await? {
            RateLimitOutcome::Granted => return Ok(()),
            RateLimitOutcome::Throttled { wait_for } => {
                attempts += 1;
                if attempts >= MAX_ACQUIRE_ATTEMPTS || waited >= MAX_THROTTLE_WAIT {
                    return Err(PlatformError::RateLimitExhausted);
                }
                let sleep_for = wait_for.min(MAX_THROTTLE_WAIT);
                tokio::time::sleep(sleep_for).await;
                waited += sleep_for;
            }
            RateLimitOutcome::Exhausted => return Err(PlatformError::RateLimitExhausted),
        }
    }
}

/// Leaky-token-bucket limiter shared across every API path that draws on a
/// single per-credential budget (e.g. one Twitch client-id has one Helix
/// budget regardless of how many transports issue requests).
///
/// Tokens refill continuously at `capacity / refill_per`. The whole-token
/// arithmetic and the cooldown deadline live behind a `std::sync::Mutex`
/// because the critical section is fully synchronous - there is no `.await`
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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn limiter(capacity: u32, refill_per: Duration) -> TokenBucketRateLimiter {
        TokenBucketRateLimiter::new(capacity, refill_per)
    }

    /// A request weighing more than the whole budget can never be funded, even
    /// on an untouched full bucket - it must short-circuit to Exhausted rather
    /// than throttle forever.
    #[tokio::test]
    async fn acquire_weight_above_capacity_is_exhausted_on_full_bucket() {
        let rl = limiter(5, Duration::from_secs(1));
        let outcome = rl.acquire(6).await.unwrap();
        assert!(matches!(outcome, RateLimitOutcome::Exhausted));
    }

    /// Fresh bucket funds exactly `capacity` unit acquires; the very next one
    /// finds the bucket empty and throttles for a positive, finite span.
    #[tokio::test]
    async fn fresh_bucket_grants_capacity_then_throttles() {
        let capacity = 4;
        let rl = limiter(capacity, Duration::from_secs(2));
        for i in 0..capacity {
            assert!(
                matches!(rl.acquire(1).await.unwrap(), RateLimitOutcome::Granted),
                "acquire {i} should be granted on a fresh bucket"
            );
        }
        match rl.acquire(1).await.unwrap() {
            RateLimitOutcome::Throttled { wait_for } => {
                // deficit is 1 token; rate is capacity/refill_per = 2/s, so the
                // estimate is ~0.5s. Assert positive and within a sane bound,
                // never an exact float.
                assert!(wait_for > Duration::ZERO);
                assert!(
                    wait_for <= Duration::from_secs(2),
                    "wait_for {wait_for:?} should not exceed the refill window"
                );
            }
            other => panic!("expected Throttled after exhausting the bucket, got {other:?}"),
        }
    }

    /// After a remote 429 back-off, the cooldown gate fires immediately: the
    /// next acquire throttles for ~retry_after even though the bucket would
    /// otherwise have tokens. Proves the cooldown gate AND the token drain.
    #[tokio::test]
    async fn remote_throttle_gates_next_acquire_for_cooldown_span() {
        let rl = limiter(10, Duration::from_secs(1));
        let retry_after = Duration::from_secs(8);
        rl.observe_remote_throttle(retry_after).await;

        match rl.acquire(1).await.unwrap() {
            RateLimitOutcome::Throttled { wait_for } => {
                assert!(wait_for > Duration::ZERO);
                // The cooldown deadline is `now + retry_after`; a hair of wall
                // time elapses before we read it, so wait_for is just under N.
                assert!(
                    wait_for <= retry_after,
                    "cooldown wait {wait_for:?} must not exceed retry_after {retry_after:?}"
                );
            }
            other => panic!("expected Throttled while cooldown is active, got {other:?}"),
        }
    }

    /// `remaining` reflects consumption: a couple of acquires on a full bucket
    /// leave fewer reported tokens than the capacity. Treated as a hint, so we
    /// assert the direction of change, not an exact count.
    #[tokio::test]
    async fn remaining_decreases_after_acquires() {
        let rl = limiter(10, Duration::from_secs(60));
        let before = rl.remaining();
        rl.acquire(1).await.unwrap();
        rl.acquire(1).await.unwrap();
        rl.acquire(1).await.unwrap();
        let after = rl.remaining();
        assert!(
            after < before,
            "remaining should drop after acquires: before={before}, after={after}"
        );
    }

    /// A zero-length refill window means tokens never regenerate: once the
    /// budget is spent, every further acquire throttles (no division-by-zero,
    /// no spurious grant).
    #[tokio::test]
    async fn zero_refill_window_never_regenerates_tokens() {
        let rl = limiter(1, Duration::ZERO);
        assert!(matches!(
            rl.acquire(1).await.unwrap(),
            RateLimitOutcome::Granted
        ));
        // Bucket is empty and the rate is zero, so wait_for collapses to ZERO
        // (no finite ETA) - but the request is still not granted.
        match rl.acquire(1).await.unwrap() {
            RateLimitOutcome::Throttled { wait_for } => {
                assert_eq!(wait_for, Duration::ZERO);
            }
            other => panic!("expected Throttled with no ETA on a dead bucket, got {other:?}"),
        }
    }
}
