use rand::RngExt;
use std::time::Duration;
use tokio::time::sleep;

const BASE_MS: u64 = 250;
const CAP_MS: u64 = 60_000;

pub(crate) fn next_backoff(attempt: u32) -> Duration {
    let shift = attempt.min(32);
    let exp = BASE_MS
        .saturating_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
        .min(CAP_MS);
    let jitter_range = exp / 4;
    let low = exp.saturating_sub(jitter_range);
    let high = exp.saturating_add(jitter_range).min(CAP_MS);
    let ms = rand::rng().random_range(low..=high);
    Duration::from_millis(ms)
}

pub(crate) async fn wait(attempt: u32) {
    sleep(next_backoff(attempt)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_zero_is_within_bounds() {
        let d = next_backoff(0);
        assert!(d <= Duration::from_millis(CAP_MS));
        assert!(d >= Duration::ZERO);
    }

    #[test]
    fn attempt_saturates_at_cap() {
        let d = next_backoff(100);
        assert!(d <= Duration::from_millis(CAP_MS));
    }

    #[test]
    fn backoff_within_jitter_bounds_for_known_attempts() {
        for attempt in 0u32..8 {
            let shift = attempt.min(32);
            let exp = BASE_MS
                .saturating_mul(1u64.checked_shl(shift).unwrap_or(u64::MAX))
                .min(CAP_MS);
            let jitter = exp / 4;
            let d = next_backoff(attempt);
            let lower = Duration::from_millis(exp.saturating_sub(jitter));
            let upper = Duration::from_millis(exp.saturating_add(jitter).min(CAP_MS));
            assert!(
                d >= lower && d <= upper,
                "attempt {attempt}: {d:?} not in [{lower:?}, {upper:?}]"
            );
        }
    }
}
