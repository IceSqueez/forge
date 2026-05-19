use std::collections::VecDeque;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use tokio::time::Instant;

const RETENTION_SECS: u64 = 60;
const CURRENT_WINDOW_SECS: u64 = 1;

pub struct BandwidthTracker {
    outbound_bytes_total: AtomicU64,
    outbound_window: Mutex<VecDeque<(Instant, u64)>>,
    peak_bps: AtomicU64,
}

impl Default for BandwidthTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BandwidthTracker {
    pub fn new() -> Self {
        Self {
            outbound_bytes_total: AtomicU64::new(0),
            outbound_window: Mutex::new(VecDeque::new()),
            peak_bps: AtomicU64::new(0),
        }
    }

    pub fn record(&self, bytes: u64) {
        self.outbound_bytes_total
            .fetch_add(bytes, Ordering::Relaxed);
        let now = Instant::now();
        let Ok(mut window) = self.outbound_window.lock() else {
            return;
        };
        window.push_back((now, bytes));
        while let Some(&(t, _)) = window.front() {
            if now.duration_since(t) > Duration::from_secs(RETENTION_SECS) {
                window.pop_front();
            } else {
                break;
            }
        }
        let current = compute_bps(&window, now);
        drop(window);

        let mut peak = self.peak_bps.load(Ordering::Relaxed);
        loop {
            if current <= peak {
                break;
            }
            match self.peak_bps.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => peak = v,
            }
        }
    }

    pub fn current_bps(&self) -> u64 {
        let now = Instant::now();
        let Ok(window) = self.outbound_window.lock() else {
            return 0;
        };
        compute_bps(&window, now)
    }

    pub fn peak(&self) -> u64 {
        self.peak_bps.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> u64 {
        self.outbound_bytes_total.load(Ordering::Relaxed)
    }
}

fn compute_bps(window: &VecDeque<(Instant, u64)>, now: Instant) -> u64 {
    window
        .iter()
        .filter(|(t, _)| now.duration_since(*t) <= Duration::from_secs(CURRENT_WINDOW_SECS))
        .map(|(_, b)| *b)
        .sum()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn record_accumulates_total_bytes() {
        let tracker = BandwidthTracker::new();
        tracker.record(500);
        tracker.record(300);
        assert_eq!(tracker.total(), 800);
    }

    #[test]
    fn current_bps_returns_bytes_within_one_second_window() {
        let tracker = BandwidthTracker::new();
        for _ in 0..60 {
            tracker.record(1000);
        }
        let bps = tracker.current_bps();
        assert!(
            (55_000..=65_000).contains(&bps),
            "expected ~60000 bps, got {bps}"
        );
    }

    #[test]
    fn current_bps_zero_on_empty() {
        let tracker = BandwidthTracker::new();
        assert_eq!(tracker.current_bps(), 0);
    }

    #[tokio::test]
    async fn peak_bps_tracks_maximum_and_does_not_decrease() {
        tokio::time::pause();
        let tracker = BandwidthTracker::new();
        for _ in 0..60 {
            tracker.record(1000);
        }
        let peak_first = tracker.peak();
        assert!(peak_first > 0, "peak should be positive after recording");

        tokio::time::advance(Duration::from_secs(2)).await;
        tracker.record(1);

        assert_eq!(
            tracker.peak(),
            peak_first,
            "peak must not decrease after a smaller burst"
        );
    }

    #[tokio::test]
    async fn entries_older_than_60s_are_trimmed_from_window() {
        tokio::time::pause();
        let tracker = BandwidthTracker::new();
        tracker.record(100_000);
        tokio::time::advance(Duration::from_secs(61)).await;
        tracker.record(1);
        let window = tracker.outbound_window.lock().unwrap();
        assert_eq!(window.len(), 1, "old entries should be trimmed after 60s");
    }
}
