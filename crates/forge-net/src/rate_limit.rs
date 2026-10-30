use async_trait::async_trait;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::cooldown::Cooldown;

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
    acquire_or_wait_within(limiter, weight, MAX_THROTTLE_WAIT).await
}

pub async fn acquire_or_wait_within(
    limiter: &dyn RateLimiter,
    weight: u32,
    max_wait: Duration,
) -> Result<(), RateLimitError> {
    let mut waited = Duration::ZERO;
    let mut attempts = 0;
    loop {
        match limiter.acquire(weight).await? {
            RateLimitOutcome::Granted => return Ok(()),
            RateLimitOutcome::Throttled { wait_for } => {
                attempts += 1;
                if attempts >= MAX_ACQUIRE_ATTEMPTS || waited >= max_wait {
                    return Err(RateLimitError::Exhausted);
                }
                let sleep_for = wait_for.min(max_wait);
                tokio::time::sleep(sleep_for).await;
                waited += sleep_for;
            }
            RateLimitOutcome::Exhausted => return Err(RateLimitError::Exhausted),
        }
    }
}

pub(crate) async fn acquire_without_wait(
    limiter: &dyn RateLimiter,
    weight: u32,
) -> Result<(), Option<Duration>> {
    match limiter.acquire(weight).await {
        Ok(RateLimitOutcome::Granted) => Ok(()),
        Ok(RateLimitOutcome::Throttled { wait_for }) => Err(Some(wait_for)),
        Ok(RateLimitOutcome::Exhausted) | Err(RateLimitError::Exhausted) => Err(None),
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
    cooldown: Cooldown,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u32, refill_per: Duration) -> Self {
        Self::new_at(capacity, refill_per, runtime_now())
    }

    pub fn new_at(capacity: u32, refill_per: Duration, now: Instant) -> Self {
        Self {
            capacity,
            refill_per,
            state: Mutex::new(BucketState {
                tokens: f64::from(capacity),
                last_refill: now,
                cooldown: Cooldown::default(),
            }),
        }
    }

    pub fn acquire_at(&self, now: Instant, weight: u32) -> RateLimitOutcome {
        if weight > self.capacity {
            return RateLimitOutcome::Exhausted;
        }
        let mut state = self.lock_state();

        let cooling_for = state.cooldown.remaining(now);
        if cooling_for > Duration::ZERO {
            return RateLimitOutcome::Throttled {
                wait_for: cooling_for,
            };
        }

        self.refill(&mut state, now);
        let needed = f64::from(weight);
        if state.tokens >= needed {
            state.tokens -= needed;
            RateLimitOutcome::Granted
        } else {
            RateLimitOutcome::Throttled {
                wait_for: self.wait_for_tokens(state.tokens, weight),
            }
        }
    }

    pub fn observe_remote_throttle_at(&self, now: Instant, retry_after: Duration) {
        let mut state = self.lock_state();
        state.cooldown.extend(now, retry_after);
        state.tokens = 0.0;
        let refill_from = state
            .cooldown
            .deadline()
            .map_or(now, |deadline| deadline.max(now));
        state.last_refill = state.last_refill.max(refill_from);
    }

    pub fn usage_at(&self, now: Instant) -> RateLimitUsage {
        let state = self.lock_state();
        let current = self.current_tokens(&state, now);
        let used = (f64::from(self.capacity) - current).round().max(0.0) as u32;
        let rate = self.tokens_per_sec();
        let resets_in = if used == 0 || rate <= 0.0 {
            Duration::ZERO
        } else {
            state
                .last_refill
                .saturating_duration_since(now)
                .saturating_add(secs_to_duration(f64::from(used) / rate))
        };
        RateLimitUsage {
            used: Some(used),
            capacity: Some(self.capacity),
            resets_in: Some(resets_in),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, BucketState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn tokens_per_sec(&self) -> f64 {
        let secs = self.refill_per.as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            f64::from(self.capacity) / secs
        }
    }

    fn refill(&self, state: &mut BucketState, now: Instant) {
        state.tokens = self.current_tokens(state, now);
        state.last_refill = state.last_refill.max(now);
    }

    fn wait_for_tokens(&self, current: f64, weight: u32) -> Duration {
        let deficit = f64::from(weight) - current;
        let rate = self.tokens_per_sec();
        if deficit <= 0.0 || rate <= 0.0 {
            return Duration::ZERO;
        }
        secs_to_duration(deficit / rate)
    }

    fn current_tokens(&self, state: &BucketState, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(state.last_refill);
        let gained = elapsed.as_secs_f64() * self.tokens_per_sec();
        (state.tokens + gained).min(f64::from(self.capacity))
    }
}

fn runtime_now() -> Instant {
    tokio::time::Instant::now().into_std()
}

fn secs_to_duration(secs: f64) -> Duration {
    Duration::try_from_secs_f64(secs).unwrap_or(Duration::MAX)
}

#[async_trait]
impl RateLimiter for TokenBucketRateLimiter {
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        Ok(self.acquire_at(runtime_now(), weight))
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.observe_remote_throttle_at(runtime_now(), retry_after);
    }

    fn usage(&self) -> RateLimitUsage {
        self.usage_at(runtime_now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::RecordingRateLimiter;

    const HOUR: Duration = Duration::from_secs(3_600);

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn bucket(capacity: u32, refill_per: Duration) -> (TokenBucketRateLimiter, Instant) {
        let t0 = Instant::now();
        (TokenBucketRateLimiter::new_at(capacity, refill_per, t0), t0)
    }

    fn wait_of(outcome: RateLimitOutcome) -> Duration {
        match outcome {
            RateLimitOutcome::Throttled { wait_for } => wait_for,
            other => panic!("expected Throttled, got {other:?}"),
        }
    }

    #[test]
    fn weight_against_a_full_bucket_grants_up_to_capacity_only() {
        let cases = [
            (4, RateLimitOutcome::Granted),
            (5, RateLimitOutcome::Granted),
            (6, RateLimitOutcome::Exhausted),
        ];
        for (weight, expected) in cases {
            let (rl, t0) = bucket(5, HOUR);
            assert_eq!(rl.acquire_at(t0, weight), expected, "weight {weight}");
        }
    }

    #[test]
    fn weighted_acquire_debits_its_full_weight() {
        let (rl, t0) = bucket(4, HOUR);
        assert_eq!(rl.acquire_at(t0, 3), RateLimitOutcome::Granted);
        assert!(matches!(
            rl.acquire_at(t0, 2),
            RateLimitOutcome::Throttled { .. }
        ));
        assert_eq!(rl.acquire_at(t0, 1), RateLimitOutcome::Granted);
    }

    #[test]
    fn throttle_wait_is_the_time_to_refill_the_deficit() {
        let (rl, t0) = bucket(10, secs(10));
        assert_eq!(rl.acquire_at(t0, 10), RateLimitOutcome::Granted);
        assert_eq!(wait_of(rl.acquire_at(t0, 3)), secs(3));
    }

    #[test]
    fn drained_bucket_refills_at_the_configured_rate() {
        let (rl, t0) = bucket(10, secs(10));
        assert_eq!(rl.acquire_at(t0, 10), RateLimitOutcome::Granted);
        assert_eq!(rl.acquire_at(t0 + secs(2), 2), RateLimitOutcome::Granted);
        assert!(matches!(
            rl.acquire_at(t0 + secs(2), 1),
            RateLimitOutcome::Throttled { .. }
        ));
    }

    #[test]
    fn refill_never_overfills_past_capacity() {
        let (rl, t0) = bucket(3, secs(3));
        assert_eq!(rl.acquire_at(t0 + HOUR, 3), RateLimitOutcome::Granted);
        assert!(matches!(
            rl.acquire_at(t0 + HOUR, 1),
            RateLimitOutcome::Throttled { .. }
        ));
    }

    #[test]
    fn earlier_now_never_moves_the_refill_clock_backwards() {
        let (rl, t0) = bucket(2, secs(2));
        assert_eq!(rl.acquire_at(t0, 2), RateLimitOutcome::Granted);
        assert_eq!(rl.acquire_at(t0 + secs(1), 1), RateLimitOutcome::Granted);
        assert!(matches!(
            rl.acquire_at(t0, 1),
            RateLimitOutcome::Throttled { .. }
        ));
        assert!(matches!(
            rl.acquire_at(t0 + secs(1), 1),
            RateLimitOutcome::Throttled { .. }
        ));
    }

    #[test]
    fn zero_refill_window_never_regenerates_tokens() {
        let (rl, t0) = bucket(1, Duration::ZERO);
        assert_eq!(rl.acquire_at(t0, 1), RateLimitOutcome::Granted);
        assert_eq!(wait_of(rl.acquire_at(t0 + HOUR, 1)), Duration::ZERO);
    }

    #[test]
    fn huge_refill_eta_saturates_without_panicking() {
        let (rl, t0) = bucket(1, Duration::MAX);
        assert_eq!(rl.acquire_at(t0, 1), RateLimitOutcome::Granted);
        assert!(wait_of(rl.acquire_at(t0, 1)) >= Duration::from_secs(u64::MAX >> 1));
        assert!(rl.usage_at(t0).resets_in.unwrap() >= Duration::from_secs(u64::MAX >> 1));
    }

    #[test]
    fn remote_throttle_gates_acquires_until_the_cooldown_ends() {
        let (rl, t0) = bucket(10, secs(1));
        let one_token = Duration::from_millis(100);
        rl.observe_remote_throttle_at(t0, secs(8));
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(3), 1)), secs(5));
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(8), 1)), one_token);
        assert_eq!(
            rl.acquire_at(t0 + secs(8) + one_token, 1),
            RateLimitOutcome::Granted
        );
    }

    #[test]
    fn long_cooldown_on_a_fast_bucket_refills_from_the_deadline_without_a_burst() {
        let one_token = Duration::from_millis(100);
        for k in 0..=10u32 {
            let (rl, t0) = bucket(10, secs(1));
            rl.observe_remote_throttle_at(t0, secs(60));
            let usage = rl.usage_at(t0 + secs(60) + one_token * k);
            assert_eq!(
                usage.used,
                Some(10 - k),
                "{k} refill steps past the deadline"
            );
        }
    }

    #[test]
    fn full_weight_acquire_at_the_cooldown_deadline_waits_a_whole_refill_window() {
        let (rl, t0) = bucket(10, secs(1));
        rl.observe_remote_throttle_at(t0, secs(60));
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(60), 10)), secs(1));
    }

    #[test]
    fn shorter_later_remote_throttle_never_moves_the_refill_clock_back() {
        let (rl, t0) = bucket(10, secs(10));
        rl.observe_remote_throttle_at(t0, secs(30));
        rl.observe_remote_throttle_at(t0 + secs(10), secs(1));
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(30), 1)), secs(1));
        assert_eq!(rl.usage_at(t0 + secs(33)).used, Some(7));
    }

    #[test]
    fn remote_throttle_hint_is_clamped_to_the_cooldown_ceiling() {
        let (rl, t0) = bucket(10, secs(1));
        rl.observe_remote_throttle_at(t0, Duration::MAX);
        assert_eq!(wait_of(rl.acquire_at(t0, 1)), crate::MAX_REMOTE_COOLDOWN);
    }

    #[test]
    fn shorter_later_remote_throttle_does_not_shorten_the_cooldown() {
        let (rl, t0) = bucket(10, secs(1));
        rl.observe_remote_throttle_at(t0, secs(30));
        rl.observe_remote_throttle_at(t0 + secs(10), secs(1));
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(10), 1)), secs(20));
    }

    #[test]
    fn longer_later_remote_throttle_extends_the_cooldown() {
        let (rl, t0) = bucket(10, secs(1));
        rl.observe_remote_throttle_at(t0, secs(5));
        rl.observe_remote_throttle_at(t0 + secs(1), secs(30));
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(1), 1)), secs(30));
    }

    #[test]
    fn remote_throttle_drains_the_bucket_so_an_expired_cooldown_releases_no_burst() {
        let (rl, t0) = bucket(10, HOUR);
        rl.observe_remote_throttle_at(t0, Duration::ZERO);
        assert!(wait_of(rl.acquire_at(t0, 1)) > Duration::ZERO);
    }

    #[test]
    fn non_extending_remote_throttle_still_drains_tokens() {
        let (rl, t0) = bucket(10, secs(100));
        rl.observe_remote_throttle_at(t0, secs(60));
        rl.observe_remote_throttle_at(t0 + secs(59), secs(1));
        assert!(matches!(
            rl.acquire_at(t0 + secs(60), 1),
            RateLimitOutcome::Throttled { .. }
        ));
    }

    #[test]
    fn remote_throttle_at_an_earlier_now_keeps_the_refill_clock() {
        let (rl, t0) = bucket(10, secs(10));
        rl.observe_remote_throttle_at(t0 + secs(5), Duration::ZERO);
        rl.observe_remote_throttle_at(t0, Duration::ZERO);
        assert_eq!(wait_of(rl.acquire_at(t0 + secs(5), 1)), secs(1));
    }

    #[tokio::test]
    async fn trait_path_feeds_the_remote_throttle_into_acquire() {
        let rl = TokenBucketRateLimiter::new(10, secs(1));
        rl.observe_remote_throttle(secs(8)).await;
        let wait = wait_of(rl.acquire(1).await.unwrap());
        assert!(wait > Duration::ZERO && wait <= secs(8));
    }

    #[test]
    fn usage_tracks_consumed_tokens_and_the_refill_eta() {
        let (rl, t0) = bucket(10, secs(10));
        let cases = [(0, 0, Duration::ZERO), (3, 3, secs(3)), (7, 10, secs(10))];
        for (weight, expected_used, expected_reset) in cases {
            if weight > 0 {
                assert_eq!(rl.acquire_at(t0, weight), RateLimitOutcome::Granted);
            }
            let usage = rl.usage_at(t0);
            assert_eq!(usage.used, Some(expected_used), "after debiting {weight}");
            assert_eq!(usage.capacity, Some(10));
            assert_eq!(
                usage.resets_in,
                Some(expected_reset),
                "after debiting {weight}"
            );
        }
    }

    #[test]
    fn usage_reset_eta_adds_the_remaining_cooldown_to_the_refill_time() {
        let cases = [
            (0, 10, secs(40)),
            (10, 10, secs(30)),
            (29, 10, secs(11)),
            (30, 10, secs(10)),
            (35, 5, secs(5)),
            (40, 0, Duration::ZERO),
        ];
        for (at, expected_used, expected_reset) in cases {
            let (rl, t0) = bucket(10, secs(10));
            rl.observe_remote_throttle_at(t0, secs(30));
            let usage = rl.usage_at(t0 + secs(at));
            assert_eq!(usage.used, Some(expected_used), "at +{at}s");
            assert_eq!(usage.resets_in, Some(expected_reset), "at +{at}s");
        }
    }

    #[test]
    fn usage_reset_eta_saturates_when_cooldown_plus_refill_overflows() {
        let (rl, t0) = bucket(1, Duration::MAX);
        rl.observe_remote_throttle_at(t0, secs(60));
        assert_eq!(rl.usage_at(t0).resets_in, Some(Duration::MAX));
    }

    #[test]
    fn usage_reflects_refill_since_the_last_acquire() {
        let (rl, t0) = bucket(10, secs(10));
        assert_eq!(rl.acquire_at(t0, 10), RateLimitOutcome::Granted);
        let usage = rl.usage_at(t0 + secs(4));
        assert_eq!(usage.used, Some(6));
        assert_eq!(usage.resets_in, Some(secs(6)));
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

    #[tokio::test(start_paused = true)]
    async fn acquire_without_wait_answers_each_outcome_in_one_attempt_without_sleeping() {
        let wait = Duration::from_millis(42_137);
        for (answer, expected) in [
            (Ok(RateLimitOutcome::Granted), Ok(())),
            (
                Ok(RateLimitOutcome::Throttled { wait_for: wait }),
                Err(Some(wait)),
            ),
            (Ok(RateLimitOutcome::Exhausted), Err(None)),
            (Err(RateLimitError::Exhausted), Err(None)),
        ] {
            let rl = RecordingRateLimiter::answering(answer);
            let started = tokio::time::Instant::now();
            assert_eq!(acquire_without_wait(&rl, 3).await, expected, "{answer:?}");
            assert_eq!(started.elapsed(), Duration::ZERO, "{answer:?}");
            assert_eq!(rl.acquired_weights(), vec![3], "{answer:?}");
        }
    }
}
