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

/// All fields `None` mean the implementation has no introspection to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RateLimitUsage {
    pub used: Option<u32>,
    pub capacity: Option<u32>,
    pub resets_in: Option<Duration>,
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, PlatformError>;
    fn remaining(&self) -> u32;
    async fn observe_remote_throttle(&self, retry_after: Duration);

    fn usage(&self) -> RateLimitUsage {
        RateLimitUsage::default()
    }
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

/// `std::sync::Mutex`, not tokio: the critical section is fully synchronous, no
/// `.await` between locking and dropping the guard.
pub struct TokenBucketRateLimiter {
    capacity: u32,
    refill_per: Duration,
    state: Mutex<BucketState>,
}

struct BucketState {
    tokens: f64,
    last_refill: Instant,
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

    fn tokens_per_sec(&self) -> f64 {
        let secs = self.refill_per.as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            self.capacity as f64 / secs
        }
    }

    /// `saturating_duration_since` avoids the Windows monotonic-clock panic near process start.
    fn refill(&self, state: &mut BucketState, now: Instant) {
        let elapsed = now.saturating_duration_since(state.last_refill);
        state.last_refill = now;
        let gained = elapsed.as_secs_f64() * self.tokens_per_sec();
        state.tokens = (state.tokens + gained).min(self.capacity as f64);
    }

    fn wait_for_tokens(&self, current: f64, weight: u32) -> Duration {
        let deficit = weight as f64 - current;
        let rate = self.tokens_per_sec();
        if deficit <= 0.0 || rate <= 0.0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(deficit / rate)
    }

    fn current_tokens(&self, state: &BucketState, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(state.last_refill);
        let gained = elapsed.as_secs_f64() * self.tokens_per_sec();
        (state.tokens + gained).min(self.capacity as f64)
    }
}

#[async_trait]
impl RateLimiter for TokenBucketRateLimiter {
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, PlatformError> {
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

    /// A hint, not a reservation: approximates the refill without mutating shared state.
    fn remaining(&self) -> u32 {
        let now = Instant::now();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.current_tokens(&state, now) as u32
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        let now = Instant::now();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.cooldown_until = Some(now + retry_after);
        state.tokens = 0.0;
        state.last_refill = now;
    }

    fn usage(&self) -> RateLimitUsage {
        let now = Instant::now();
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = self.current_tokens(&state, now);
        let used = (self.capacity as f64 - current).round().max(0.0) as u32;
        let rate = self.tokens_per_sec();
        let resets_in = if used == 0 || rate <= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(used as f64 / rate)
        };
        RateLimitUsage {
            used: Some(used),
            capacity: Some(self.capacity),
            resets_in: Some(resets_in),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn limiter(capacity: u32, refill_per: Duration) -> TokenBucketRateLimiter {
        TokenBucketRateLimiter::new(capacity, refill_per)
    }

    #[tokio::test]
    async fn acquire_weight_above_capacity_is_exhausted_on_full_bucket() {
        let rl = limiter(5, Duration::from_secs(1));
        let outcome = rl.acquire(6).await.unwrap();
        assert!(matches!(outcome, RateLimitOutcome::Exhausted));
    }

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
                assert!(wait_for > Duration::ZERO);
                assert!(
                    wait_for <= Duration::from_secs(2),
                    "wait_for {wait_for:?} should not exceed the refill window"
                );
            }
            other => panic!("expected Throttled after exhausting the bucket, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn remote_throttle_gates_next_acquire_for_cooldown_span() {
        let rl = limiter(10, Duration::from_secs(1));
        let retry_after = Duration::from_secs(8);
        rl.observe_remote_throttle(retry_after).await;

        match rl.acquire(1).await.unwrap() {
            RateLimitOutcome::Throttled { wait_for } => {
                assert!(wait_for > Duration::ZERO);
                assert!(
                    wait_for <= retry_after,
                    "cooldown wait {wait_for:?} must not exceed retry_after {retry_after:?}"
                );
            }
            other => panic!("expected Throttled while cooldown is active, got {other:?}"),
        }
    }

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

    #[tokio::test]
    async fn zero_refill_window_never_regenerates_tokens() {
        let rl = limiter(1, Duration::ZERO);
        assert!(matches!(
            rl.acquire(1).await.unwrap(),
            RateLimitOutcome::Granted
        ));
        match rl.acquire(1).await.unwrap() {
            RateLimitOutcome::Throttled { wait_for } => {
                assert_eq!(wait_for, Duration::ZERO);
            }
            other => panic!("expected Throttled with no ETA on a dead bucket, got {other:?}"),
        }
    }
}
