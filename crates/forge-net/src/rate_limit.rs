use async_trait::async_trait;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RateLimitError {
    #[error("client-side rate limit exhausted; no budget remaining")]
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitOutcome {
    Granted,
    Throttled { wait_for: Duration },
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RateLimitUsage {
    pub used: Option<u32>,
    pub capacity: Option<u32>,
    pub resets_in: Option<Duration>,
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, RateLimitError>;
    async fn observe_remote_throttle(&self, retry_after: Duration);

    fn usage(&self) -> RateLimitUsage {
        RateLimitUsage::default()
    }
}

pub const MAX_THROTTLE_WAIT: Duration = Duration::from_secs(10);
pub const MAX_ACQUIRE_ATTEMPTS: u32 = 3;
pub const DEFAULT_RETRY_AFTER_SECS: u32 = 30;

pub async fn acquire_or_wait(limiter: &dyn RateLimiter, weight: u32) -> Result<(), RateLimitError> {
    let mut waited = Duration::ZERO;
    let mut attempts = 0;
    loop {
        match limiter.acquire(weight).await? {
            RateLimitOutcome::Granted => return Ok(()),
            RateLimitOutcome::Throttled { wait_for } => {
                attempts += 1;
                if attempts >= MAX_ACQUIRE_ATTEMPTS || waited >= MAX_THROTTLE_WAIT {
                    return Err(RateLimitError::Exhausted);
                }
                let sleep_for = wait_for.min(MAX_THROTTLE_WAIT);
                tokio::time::sleep(sleep_for).await;
                waited += sleep_for;
            }
            RateLimitOutcome::Exhausted => return Err(RateLimitError::Exhausted),
        }
    }
}

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
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
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
    use crate::test_support::RecordingRateLimiter;

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
        let window = Duration::from_secs(3_600);
        let rl = limiter(capacity, window);
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
                    wait_for <= window,
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

    #[test]
    fn usage_on_fresh_bucket_reports_zero_used_and_no_reset() {
        let rl = limiter(10, Duration::from_secs(60));
        let usage = rl.usage();
        assert_eq!(usage.used, Some(0));
        assert_eq!(usage.resets_in, Some(Duration::ZERO));
    }

    #[tokio::test]
    async fn usage_after_acquires_counts_consumed_against_capacity() {
        let rl = limiter(10, Duration::from_secs(3_600));
        for _ in 0..3 {
            rl.acquire(1).await.unwrap();
        }
        let usage = rl.usage();
        assert_eq!(usage.used, Some(3));
        assert_eq!(usage.capacity, Some(10));
        let resets_in = usage.resets_in.unwrap();
        assert!(resets_in > Duration::ZERO);
        assert!(
            resets_in <= Duration::from_secs(3_600),
            "reset ETA {resets_in:?} must not exceed the refill window"
        );
    }

    #[tokio::test]
    async fn usage_on_exhausted_bucket_reports_full_capacity_and_window_reset() {
        let window = Duration::from_secs(8 * 3_600);
        let rl = limiter(4, window);
        for _ in 0..4 {
            rl.acquire(1).await.unwrap();
        }
        let usage = rl.usage();
        assert_eq!(usage.used, Some(4));
        assert_eq!(usage.resets_in, Some(window));
    }

    struct NoIntrospectionLimiter;

    #[async_trait]
    impl RateLimiter for NoIntrospectionLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
            Ok(RateLimitOutcome::Granted)
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    #[test]
    fn default_usage_offers_no_introspection() {
        let usage = NoIntrospectionLimiter.usage();
        assert!(usage.used.is_none());
        assert!(usage.capacity.is_none());
        assert!(usage.resets_in.is_none());
    }

    #[tokio::test]
    async fn acquire_weight_equal_to_capacity_is_granted_on_full_bucket() {
        let rl = limiter(5, Duration::from_secs(3_600));
        assert!(matches!(
            rl.acquire(5).await.unwrap(),
            RateLimitOutcome::Granted
        ));
    }

    #[tokio::test]
    async fn weighted_acquire_debits_its_full_weight() {
        let rl = limiter(4, Duration::from_secs(3_600));
        rl.acquire(3).await.unwrap();
        assert!(matches!(
            rl.acquire(2).await.unwrap(),
            RateLimitOutcome::Throttled { .. }
        ));
        assert!(matches!(
            rl.acquire(1).await.unwrap(),
            RateLimitOutcome::Granted
        ));
    }

    #[tokio::test]
    async fn remote_throttle_drains_the_bucket_so_an_expired_cooldown_releases_no_burst() {
        let rl = limiter(10, Duration::from_secs(3_600));
        rl.observe_remote_throttle(Duration::ZERO).await;
        assert!(matches!(
            rl.acquire(1).await.unwrap(),
            RateLimitOutcome::Throttled { wait_for } if wait_for > Duration::ZERO
        ));
    }

    fn throttled(secs: u64) -> Result<RateLimitOutcome, RateLimitError> {
        Ok(RateLimitOutcome::Throttled {
            wait_for: Duration::from_secs(secs),
        })
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_returns_at_once_when_granted() {
        let rl = RecordingRateLimiter::granting();
        let started = tokio::time::Instant::now();
        assert_eq!(acquire_or_wait(&rl, 1).await, Ok(()));
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_sleeps_the_throttle_wait_then_retries() {
        let rl = RecordingRateLimiter::granting().with_script([throttled(2)]);
        let started = tokio::time::Instant::now();
        assert_eq!(acquire_or_wait(&rl, 1).await, Ok(()));
        assert_eq!(started.elapsed(), Duration::from_secs(2));
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_retries_with_the_callers_weight() {
        let rl = RecordingRateLimiter::granting().with_script([throttled(1)]);
        acquire_or_wait(&rl, 7).await.unwrap();
        assert_eq!(rl.acquired_weights(), vec![7, 7]);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_clamps_a_single_throttle_sleep_to_the_wait_ceiling() {
        let rl = RecordingRateLimiter::granting().with_script([throttled(600)]);
        let started = tokio::time::Instant::now();
        assert_eq!(acquire_or_wait(&rl, 1).await, Ok(()));
        assert_eq!(started.elapsed(), MAX_THROTTLE_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_gives_up_after_the_attempt_cap() {
        let rl = RecordingRateLimiter::answering(throttled(1));
        assert_eq!(
            acquire_or_wait(&rl, 1).await,
            Err(RateLimitError::Exhausted)
        );
        assert_eq!(rl.acquire_count(), MAX_ACQUIRE_ATTEMPTS as usize);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_gives_up_once_the_wait_budget_is_spent() {
        let rl = RecordingRateLimiter::granting().with_script([throttled(10), throttled(1)]);
        assert_eq!(
            acquire_or_wait(&rl, 1).await,
            Err(RateLimitError::Exhausted)
        );
        assert_eq!(rl.acquire_count(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_keeps_retrying_just_under_the_wait_budget() {
        let rl = RecordingRateLimiter::granting().with_script([throttled(9), throttled(1)]);
        assert_eq!(acquire_or_wait(&rl, 1).await, Ok(()));
        assert_eq!(rl.acquire_count(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_fails_without_sleeping_on_an_exhausted_outcome() {
        let rl = RecordingRateLimiter::exhausted();
        let started = tokio::time::Instant::now();
        assert_eq!(
            acquire_or_wait(&rl, 1).await,
            Err(RateLimitError::Exhausted)
        );
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn acquire_or_wait_propagates_a_limiter_error_without_retrying() {
        let rl = RecordingRateLimiter::failing();
        assert_eq!(
            acquire_or_wait(&rl, 1).await,
            Err(RateLimitError::Exhausted)
        );
        assert_eq!(rl.acquire_count(), 1);
    }
}
