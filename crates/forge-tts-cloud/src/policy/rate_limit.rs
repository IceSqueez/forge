use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter, TokenBucketRateLimiter};

const EXHAUSTED_THRESHOLD: Duration = Duration::from_secs(60);
const SYNTHESIS_BURST: u32 = 10;
const SYNTHESIS_REFILL_WINDOW: Duration = Duration::from_secs(1);

pub struct SynthesisRateLimiter {
    bucket: TokenBucketRateLimiter,
}

impl SynthesisRateLimiter {
    pub fn new() -> Self {
        Self::new_at(runtime_now())
    }

    pub fn new_at(now: Instant) -> Self {
        Self {
            bucket: TokenBucketRateLimiter::new_at(SYNTHESIS_BURST, SYNTHESIS_REFILL_WINDOW, now),
        }
    }

    pub fn acquire_at(&self, now: Instant, weight: u32) -> RateLimitOutcome {
        match self.bucket.acquire_at(now, weight) {
            RateLimitOutcome::Throttled { wait_for } if wait_for > EXHAUSTED_THRESHOLD => {
                RateLimitOutcome::Exhausted
            }
            outcome => outcome,
        }
    }

    pub fn observe_remote_throttle_at(&self, now: Instant, retry_after: Duration) {
        self.bucket.observe_remote_throttle_at(now, retry_after);
    }
}

impl Default for SynthesisRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn runtime_now() -> Instant {
    tokio::time::Instant::now().into_std()
}

#[async_trait]
impl RateLimiter for SynthesisRateLimiter {
    async fn acquire(&self, weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        Ok(self.acquire_at(runtime_now(), weight))
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.observe_remote_throttle_at(runtime_now(), retry_after);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn limiter() -> (SynthesisRateLimiter, Instant) {
        let t0 = Instant::now();
        (SynthesisRateLimiter::new_at(t0), t0)
    }

    fn wait_of(outcome: RateLimitOutcome) -> Duration {
        match outcome {
            RateLimitOutcome::Throttled { wait_for } => wait_for,
            other => panic!("expected Throttled, got {other:?}"),
        }
    }

    #[test]
    fn weight_against_a_fresh_limiter_grants_up_to_the_burst_only() {
        let cases = [
            (1, RateLimitOutcome::Granted),
            (SYNTHESIS_BURST, RateLimitOutcome::Granted),
            (SYNTHESIS_BURST + 1, RateLimitOutcome::Exhausted),
        ];
        for (weight, expected) in cases {
            let (lim, t0) = limiter();
            assert_eq!(lim.acquire_at(t0, weight), expected, "weight {weight}");
        }
    }

    #[test]
    fn acquire_debits_its_full_weight() {
        let (lim, t0) = limiter();
        assert_eq!(lim.acquire_at(t0, 7), RateLimitOutcome::Granted);
        assert!(matches!(
            lim.acquire_at(t0, 4),
            RateLimitOutcome::Throttled { .. }
        ));
        assert_eq!(lim.acquire_at(t0, 3), RateLimitOutcome::Granted);
    }

    #[test]
    fn remote_hint_up_to_the_threshold_throttles_and_longer_exhausts() {
        let cases = [
            (secs(10), RateLimitOutcome::Throttled { wait_for: secs(10) }),
            (
                EXHAUSTED_THRESHOLD,
                RateLimitOutcome::Throttled {
                    wait_for: EXHAUSTED_THRESHOLD,
                },
            ),
            (EXHAUSTED_THRESHOLD + secs(1), RateLimitOutcome::Exhausted),
            (Duration::MAX, RateLimitOutcome::Exhausted),
        ];
        for (hint, expected) in cases {
            let (lim, t0) = limiter();
            lim.observe_remote_throttle_at(t0, hint);
            assert_eq!(lim.acquire_at(t0, 1), expected, "hint {hint:?}");
        }
    }

    #[test]
    fn exhausted_wait_turns_throttled_once_inside_the_threshold() {
        let (lim, t0) = limiter();
        lim.observe_remote_throttle_at(t0, secs(90));
        assert_eq!(lim.acquire_at(t0, 1), RateLimitOutcome::Exhausted);
        assert_eq!(wait_of(lim.acquire_at(t0 + secs(30), 1)), secs(60));
    }

    #[test]
    fn grants_again_one_refill_interval_after_the_floor_passes() {
        let (lim, t0) = limiter();
        let one_token = SYNTHESIS_REFILL_WINDOW / SYNTHESIS_BURST;
        lim.observe_remote_throttle_at(t0, secs(5));
        let just_before = t0 + secs(5) - Duration::from_millis(1);
        assert_eq!(
            wait_of(lim.acquire_at(just_before, 1)),
            Duration::from_millis(1)
        );
        assert_eq!(wait_of(lim.acquire_at(t0 + secs(5), 1)), one_token);
        assert_eq!(
            lim.acquire_at(t0 + secs(5) + one_token, 1),
            RateLimitOutcome::Granted
        );
    }

    #[test]
    fn shorter_later_hint_never_shortens_the_floor() {
        let (lim, t0) = limiter();
        lim.observe_remote_throttle_at(t0, secs(30));
        lim.observe_remote_throttle_at(t0 + secs(1), secs(2));
        assert_eq!(wait_of(lim.acquire_at(t0 + secs(10), 1)), secs(20));
    }

    #[test]
    fn longer_later_hint_extends_the_floor_past_the_threshold() {
        let (lim, t0) = limiter();
        lim.observe_remote_throttle_at(t0, secs(5));
        lim.observe_remote_throttle_at(t0 + secs(1), secs(90));
        assert_eq!(lim.acquire_at(t0 + secs(1), 1), RateLimitOutcome::Exhausted);
    }

    #[tokio::test(start_paused = true)]
    async fn trait_path_reads_the_runtime_clock() {
        let lim = SynthesisRateLimiter::new();
        lim.observe_remote_throttle(secs(5)).await;
        assert_eq!(
            lim.acquire(1).await.unwrap(),
            RateLimitOutcome::Throttled { wait_for: secs(5) }
        );
        let one_token = SYNTHESIS_REFILL_WINDOW / SYNTHESIS_BURST;
        tokio::time::advance(secs(5)).await;
        assert_eq!(
            lim.acquire(1).await.unwrap(),
            RateLimitOutcome::Throttled {
                wait_for: one_token
            }
        );
        tokio::time::advance(one_token).await;
        assert_eq!(lim.acquire(1).await.unwrap(), RateLimitOutcome::Granted);
    }
}
