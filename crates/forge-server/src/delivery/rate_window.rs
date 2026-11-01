use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

pub(crate) struct RateWindow {
    span: Duration,
    samples: VecDeque<(Instant, u64)>,
    total: u64,
}

impl RateWindow {
    pub(crate) fn new(span: Duration) -> Self {
        Self {
            span,
            samples: VecDeque::new(),
            total: 0,
        }
    }

    pub(crate) fn push(&mut self, at: Instant, amount: u64) {
        self.samples.push_back((at, amount));
        self.total = self.total.saturating_add(amount);
    }

    pub(crate) fn total_at(&mut self, now: Instant) -> u64 {
        while let Some(&(at, amount)) = self.samples.front() {
            if now.duration_since(at) <= self.span {
                break;
            }
            self.samples.pop_front();
            self.total = self.total.saturating_sub(amount);
        }
        self.total
    }

    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.samples.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const SPAN: Duration = Duration::from_secs(10);
    const STEP: Duration = Duration::from_secs(1);

    #[test]
    fn an_empty_window_totals_zero() {
        let mut window = RateWindow::new(SPAN);

        assert_eq!(window.total_at(Instant::now()), 0);
        assert_eq!(window.retained(), 0);
    }

    #[test]
    fn the_window_totals_only_samples_inside_the_span() {
        for (label, sampled_at, read_at, total, retained) in [
            ("samples_inside_the_span_add_up", STEP, STEP * 2, 12, 2),
            (
                "a_sample_older_than_the_span_is_dropped_and_newer_ones_stay",
                SPAN,
                SPAN + STEP,
                7,
                1,
            ),
            (
                "a_window_that_aged_out_entirely_totals_zero",
                STEP,
                SPAN * 3,
                0,
                0,
            ),
        ] {
            let start = Instant::now();
            let mut window = RateWindow::new(SPAN);
            window.push(start, 5);
            window.push(start + sampled_at, 7);

            assert_eq!(window.total_at(start + read_at), total, "{label}");
            assert_eq!(window.retained(), retained, "{label}");
        }
    }

    #[test]
    fn a_sample_exactly_one_span_old_still_counts() {
        let start = Instant::now();
        let mut window = RateWindow::new(SPAN);
        window.push(start, 5);

        assert_eq!(window.total_at(start + SPAN), 5);
    }

    #[test]
    fn totals_saturate_instead_of_overflowing() {
        let start = Instant::now();
        let mut window = RateWindow::new(SPAN);
        window.push(start, u64::MAX);
        window.push(start + STEP, 1);

        assert_eq!(window.total_at(start + STEP), u64::MAX);
    }
}
