use std::collections::HashMap;
use std::time::{Duration, Instant};

use forge_net::{Cooldown, MAX_REMOTE_COOLDOWN};

#[derive(Debug, Clone)]
pub(crate) struct DiscordBucket {
    pub(crate) limit: u32,
    pub(crate) remaining: u32,
    pub(crate) reset_after: Duration,
    pub(crate) updated_at: Instant,
    cooldown: Cooldown,
}

#[derive(Debug)]
pub(crate) enum RateLimitOutcome {
    Granted,
    Throttled { wait_for: Duration },
}

impl DiscordBucket {
    pub(crate) fn new_at(now: Instant) -> Self {
        Self {
            limit: 0,
            remaining: 0,
            reset_after: Duration::ZERO,
            updated_at: now,
            cooldown: Cooldown::default(),
        }
    }

    pub(crate) fn check(&mut self, now: Instant) -> RateLimitOutcome {
        let cooling_for = self.cooldown.remaining(now);
        if cooling_for > Duration::ZERO {
            return RateLimitOutcome::Throttled {
                wait_for: cooling_for,
            };
        }
        if self.limit == 0 {
            return RateLimitOutcome::Granted;
        }
        let mut elapsed = now.saturating_duration_since(self.updated_at);
        if elapsed >= self.reset_after {
            self.remaining = self.limit;
            self.updated_at = self.updated_at.max(now);
            elapsed = Duration::ZERO;
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
        now: Instant,
        limit: u32,
        remaining: u32,
        reset_after_secs: f64,
    ) {
        self.limit = limit;
        self.remaining = remaining;
        self.reset_after = reset_window(reset_after_secs);
        self.updated_at = now;
    }

    pub(crate) fn observe_remote_throttle(&mut self, now: Instant, retry_after: Duration) {
        self.cooldown.extend(now, retry_after);
        self.remaining = 0;
        self.reset_after = self.cooldown.remaining(now);
        self.updated_at = now;
    }

    pub(crate) fn reset_hint_secs(&self, now: Instant) -> Option<f64> {
        let window_left = if self.remaining == 0 && self.limit > 0 {
            self.reset_after
                .saturating_sub(now.saturating_duration_since(self.updated_at))
        } else {
            Duration::ZERO
        };
        let wait = window_left.max(self.cooldown.remaining(now));
        (wait > Duration::ZERO).then_some(wait.as_secs_f64())
    }
}

fn reset_window(reset_after_secs: f64) -> Duration {
    if reset_after_secs.is_nan() || reset_after_secs <= 0.0 {
        return Duration::ZERO;
    }
    Duration::try_from_secs_f64(reset_after_secs)
        .unwrap_or(Duration::MAX)
        .min(MAX_REMOTE_COOLDOWN)
}

pub(crate) struct DiscordRateLimiter {
    buckets: HashMap<String, DiscordBucket>,
    global: Cooldown,
}

impl DiscordRateLimiter {
    pub(crate) fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            global: Cooldown::default(),
        }
    }

    fn bucket(&mut self, now: Instant, name: &str) -> &mut DiscordBucket {
        self.buckets
            .entry(name.to_owned())
            .or_insert_with(|| DiscordBucket::new_at(now))
    }

    pub(crate) fn acquire(&mut self, now: Instant, name: &str) -> RateLimitOutcome {
        self.bucket(now, name).check(now)
    }

    pub(crate) fn record_response(
        &mut self,
        now: Instant,
        name: &str,
        limit: u32,
        remaining: u32,
        reset_after_secs: f64,
    ) {
        self.bucket(now, name)
            .update_from_headers(now, limit, remaining, reset_after_secs);
    }

    pub(crate) fn observe_global_throttle(&mut self, now: Instant, duration: Duration) {
        self.global.extend(now, duration);
    }

    pub(crate) fn observe_remote_throttle(
        &mut self,
        now: Instant,
        name: &str,
        retry_after: Duration,
    ) {
        self.bucket(now, name)
            .observe_remote_throttle(now, retry_after);
    }

    pub(crate) fn global_wait_duration(&self, now: Instant) -> Option<Duration> {
        let wait = self.global.remaining(now);
        (wait > Duration::ZERO).then_some(wait)
    }

    pub(crate) fn budget(&self, name: &str) -> (u64, u64) {
        self.buckets
            .get(name)
            .map_or((0, 0), |b| (b.remaining as u64, b.limit as u64))
    }

    pub(crate) fn reset_hint_secs(&self, now: Instant, name: &str) -> Option<f64> {
        self.buckets.get(name)?.reset_hint_secs(now)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn wait_of(outcome: RateLimitOutcome) -> Duration {
        match outcome {
            RateLimitOutcome::Throttled { wait_for } => wait_for,
            RateLimitOutcome::Granted => panic!("expected Throttled, got Granted"),
        }
    }

    fn granted(outcome: RateLimitOutcome) -> bool {
        matches!(outcome, RateLimitOutcome::Granted)
    }

    #[test]
    fn bucket_without_header_state_grants() {
        let t0 = Instant::now();
        assert!(granted(DiscordBucket::new_at(t0).check(t0)));
    }

    #[test]
    fn bucket_with_remaining_grants_and_debits_one() {
        let t0 = Instant::now();
        let mut b = DiscordBucket::new_at(t0);
        b.update_from_headers(t0, 5, 1, 3600.0);
        assert!(granted(b.check(t0)));
        assert_eq!(wait_of(b.check(t0)), secs(3600));
    }

    #[test]
    fn exhausted_bucket_waits_out_the_header_window_then_refills() {
        let t0 = Instant::now();
        let mut b = DiscordBucket::new_at(t0);
        b.update_from_headers(t0, 5, 0, 10.0);
        assert_eq!(wait_of(b.check(t0 + secs(4))), secs(6));
        assert!(granted(b.check(t0 + secs(10))));
    }

    #[test]
    fn stale_refill_opens_one_window_counted_from_the_refill_moment() {
        let t0 = Instant::now();
        let cases = [
            ("refill at the window edge", secs(10)),
            ("refill after a long idle", secs(25)),
            ("refill after a day idle", secs(86_400)),
        ];
        for (label, refill_at) in cases {
            let mut b = DiscordBucket::new_at(t0);
            b.update_from_headers(t0, 3, 0, 10.0);
            let r = t0 + refill_at;
            for _ in 0..3 {
                assert!(granted(b.check(r)), "{label}");
            }
            assert_eq!(wait_of(b.check(r)), secs(10), "{label}");
            assert_eq!(wait_of(b.check(r + secs(5))), secs(5), "{label}");
            assert_eq!(b.reset_hint_secs(r + secs(5)), Some(5.0), "{label}");
            assert!(granted(b.check(r + secs(10))), "{label}");
        }
    }

    #[test]
    fn earlier_now_after_a_refill_never_moves_the_window_back() {
        let t0 = Instant::now();
        let mut b = DiscordBucket::new_at(t0);
        b.update_from_headers(t0, 2, 0, 10.0);
        assert!(granted(b.check(t0 + secs(20))));
        assert!(granted(b.check(t0 + secs(12))));
        assert_eq!(wait_of(b.check(t0 + secs(12))), secs(10));
        assert_eq!(wait_of(b.check(t0 + secs(25))), secs(5));
        assert!(granted(b.check(t0 + secs(30))));
    }

    #[test]
    fn zero_reset_window_refills_on_every_check() {
        let t0 = Instant::now();
        let mut b = DiscordBucket::new_at(t0);
        b.update_from_headers(t0, 2, 0, 0.0);
        for n in 0..5 {
            assert!(granted(b.check(t0)), "check {n}");
        }
        assert_eq!(b.reset_hint_secs(t0), None);
    }

    #[test]
    fn remote_throttle_gates_a_bucket_with_no_header_state() {
        let t0 = Instant::now();
        let mut b = DiscordBucket::new_at(t0);
        b.observe_remote_throttle(t0, secs(5));
        assert_eq!(wait_of(b.check(t0 + secs(1))), secs(4));
        assert!(granted(b.check(t0 + secs(5))));
    }

    #[test]
    fn limiter_remote_throttle_gates_an_unseen_webhook() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.observe_remote_throttle(t0, "alerts", secs(5));
        assert_eq!(wait_of(rl.acquire(t0, "alerts")), secs(5));
        assert!(granted(rl.acquire(t0, "clips")));
    }

    #[test]
    fn header_update_does_not_cancel_an_active_remote_throttle() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.observe_remote_throttle(t0, "alerts", secs(10));
        rl.record_response(t0 + secs(1), "alerts", 5, 5, 0.5);
        assert_eq!(wait_of(rl.acquire(t0 + secs(2), "alerts")), secs(8));
    }

    #[test]
    fn shorter_later_remote_throttle_does_not_shorten_the_bucket_cooldown() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.observe_remote_throttle(t0, "alerts", secs(30));
        rl.observe_remote_throttle(t0 + secs(10), "alerts", secs(1));
        assert_eq!(wait_of(rl.acquire(t0 + secs(10), "alerts")), secs(20));
    }

    #[test]
    fn remote_throttle_zeroes_the_reported_budget() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.record_response(t0, "alerts", 5, 3, 1.0);
        rl.observe_remote_throttle(t0, "alerts", secs(2));
        assert_eq!(rl.budget("alerts"), (0, 5));
    }

    #[test]
    fn acquire_uses_the_per_webhook_bucket() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.record_response(t0, "alerts", 5, 2, 3600.0);
        rl.record_response(t0, "clips", 5, 0, 3600.0);
        assert!(granted(rl.acquire(t0, "alerts")));
        assert!(!granted(rl.acquire(t0, "clips")));
    }

    #[test]
    fn budget_reports_remaining_and_limit_or_zero_for_unknown() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.record_response(t0, "alerts", 5, 3, 1.0);
        assert_eq!(rl.budget("alerts"), (3, 5));
        assert_eq!(rl.budget("unknown"), (0, 0));
    }

    type HintCase = (&'static str, Option<u64>, Option<(u32, f64)>, Option<f64>);

    #[test]
    fn reset_hint_is_the_longer_of_window_and_cooldown() {
        let t0 = Instant::now();
        let cases: [HintCase; 6] = [
            ("no state", None, None, None),
            ("budget left, no cooldown", None, Some((3, 30.0)), None),
            ("exhausted window only", None, Some((0, 30.0)), Some(30.0)),
            ("cooldown only", Some(20), None, Some(20.0)),
            (
                "window longer than cooldown",
                Some(10),
                Some((0, 30.0)),
                Some(30.0),
            ),
            (
                "cooldown longer than window",
                Some(20),
                Some((0, 5.0)),
                Some(20.0),
            ),
        ];
        for (label, cooldown, headers, expected) in cases {
            let mut rl = DiscordRateLimiter::new();
            rl.record_response(t0, "alerts", 0, 0, 0.0);
            if let Some(cooldown) = cooldown {
                rl.observe_remote_throttle(t0, "alerts", secs(cooldown));
            }
            if let Some((remaining, window)) = headers {
                rl.record_response(t0, "alerts", 5, remaining, window);
            }
            assert_eq!(rl.reset_hint_secs(t0, "alerts"), expected, "{label}");
        }
    }

    #[test]
    fn reset_hint_keeps_the_cooldown_when_budget_is_left() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.observe_remote_throttle(t0, "alerts", secs(20));
        rl.record_response(t0, "alerts", 5, 3, 1.0);
        assert_eq!(rl.reset_hint_secs(t0 + secs(5), "alerts"), Some(15.0));
    }

    #[test]
    fn reset_hint_counts_down_and_clears_after_expiry() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.record_response(t0, "alerts", 5, 0, 10.0);
        assert_eq!(rl.reset_hint_secs(t0 + secs(4), "alerts"), Some(6.0));
        assert_eq!(rl.reset_hint_secs(t0 + secs(10), "alerts"), None);
        assert_eq!(rl.reset_hint_secs(t0, "unknown"), None);
    }

    #[test]
    fn header_window_is_clamped_and_never_panics() {
        let t0 = Instant::now();
        let cases = [
            (f64::NAN, Duration::ZERO),
            (f64::NEG_INFINITY, Duration::ZERO),
            (-1.0, Duration::ZERO),
            (0.0, Duration::ZERO),
            (0.25, Duration::from_millis(250)),
            (3600.0, MAX_REMOTE_COOLDOWN),
            (3600.5, MAX_REMOTE_COOLDOWN),
            (1e300, MAX_REMOTE_COOLDOWN),
            (f64::INFINITY, MAX_REMOTE_COOLDOWN),
        ];
        for (window, expected) in cases {
            let mut b = DiscordBucket::new_at(t0);
            b.update_from_headers(t0, 5, 0, window);
            assert_eq!(b.reset_after, expected, "window {window}");
        }
    }

    #[test]
    fn non_finite_or_negative_window_does_not_lock_the_bucket() {
        let t0 = Instant::now();
        for window in [f64::NAN, f64::NEG_INFINITY, -1.0] {
            let mut b = DiscordBucket::new_at(t0);
            b.update_from_headers(t0, 5, 0, window);
            assert!(granted(b.check(t0)), "window {window}");
        }
    }

    #[test]
    fn global_throttle_extends_monotonically_and_clamps() {
        let t0 = Instant::now();
        let later = t0 + secs(1);
        let cases = [
            ("shorter later hint", secs(30), secs(5), Some(secs(29))),
            ("longer later hint", secs(30), secs(60), Some(secs(60))),
            (
                "huge later hint",
                secs(30),
                Duration::MAX,
                Some(MAX_REMOTE_COOLDOWN),
            ),
            ("zero hints", Duration::ZERO, Duration::ZERO, None),
        ];
        for (label, first, second, expected) in cases {
            let mut rl = DiscordRateLimiter::new();
            rl.observe_global_throttle(t0, first);
            rl.observe_global_throttle(later, second);
            assert_eq!(rl.global_wait_duration(later), expected, "{label}");
        }
    }

    #[test]
    fn global_throttle_clears_once_the_deadline_passes() {
        let t0 = Instant::now();
        let mut rl = DiscordRateLimiter::new();
        rl.observe_global_throttle(t0, secs(30));
        assert_eq!(rl.global_wait_duration(t0 + secs(29)), Some(secs(1)));
        assert_eq!(rl.global_wait_duration(t0 + secs(30)), None);
    }
}
