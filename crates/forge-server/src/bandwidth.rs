use forge_types::RecoverMutex;
use std::collections::VecDeque;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use tokio::time::Instant;

const CURRENT_WINDOW: Duration = Duration::from_secs(1);

#[derive(Default)]
struct OutboundWindow {
    frames: VecDeque<(Instant, u64)>,
    bytes: u64,
}

impl OutboundWindow {
    fn push(&mut self, at: Instant, bytes: u64) {
        self.frames.push_back((at, bytes));
        self.bytes = self.bytes.saturating_add(bytes);
    }

    fn expire(&mut self, now: Instant) -> u64 {
        while let Some(&(at, bytes)) = self.frames.front() {
            if now.duration_since(at) <= CURRENT_WINDOW {
                break;
            }
            self.frames.pop_front();
            self.bytes = self.bytes.saturating_sub(bytes);
        }
        self.bytes
    }
}

#[derive(Default)]
pub(crate) struct BandwidthTracker {
    outbound_bytes_total: AtomicU64,
    outbound_window: Mutex<OutboundWindow>,
    peak_bytes_per_second: AtomicU64,
}

impl BandwidthTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record(&self, bytes: u64) {
        self.outbound_bytes_total
            .fetch_add(bytes, Ordering::Relaxed);
        let now = Instant::now();
        let current = {
            let mut window = self.outbound_window.lock_recover();
            window.push(now, bytes);
            window.expire(now)
        };
        self.peak_bytes_per_second
            .fetch_max(current, Ordering::Relaxed);
    }

    pub(crate) fn current_bytes_per_second(&self) -> u64 {
        self.outbound_window.lock_recover().expire(Instant::now())
    }

    pub(crate) fn peak_bytes_per_second(&self) -> u64 {
        self.peak_bytes_per_second.load(Ordering::Relaxed)
    }

    pub(crate) fn total(&self) -> u64 {
        self.outbound_bytes_total.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
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

    #[tokio::test(start_paused = true)]
    async fn current_bps_sums_the_bytes_inside_the_one_second_window() {
        let tracker = BandwidthTracker::new();
        for _ in 0..60 {
            tracker.record(1000);
        }
        assert_eq!(tracker.current_bytes_per_second(), 60_000);
    }

    #[test]
    fn current_bps_zero_on_empty() {
        let tracker = BandwidthTracker::new();
        assert_eq!(tracker.current_bytes_per_second(), 0);
    }

    #[tokio::test]
    async fn peak_bps_tracks_maximum_and_does_not_decrease() {
        tokio::time::pause();
        let tracker = BandwidthTracker::new();
        for _ in 0..60 {
            tracker.record(1000);
        }
        let peak_first = tracker.peak_bytes_per_second();
        assert!(peak_first > 0, "peak should be positive after recording");

        tokio::time::advance(Duration::from_secs(2)).await;
        tracker.record(1);

        assert_eq!(
            tracker.peak_bytes_per_second(),
            peak_first,
            "peak must not decrease after a smaller burst"
        );
    }

    #[tokio::test]
    async fn frames_past_the_current_window_are_trimmed() {
        tokio::time::pause();
        let tracker = BandwidthTracker::new();
        tracker.record(100_000);
        tokio::time::advance(Duration::from_secs(61)).await;
        tracker.record(1);
        let window = tracker.outbound_window.lock().unwrap();
        assert_eq!(window.frames.len(), 1, "expired frames leave the window");
    }
}
