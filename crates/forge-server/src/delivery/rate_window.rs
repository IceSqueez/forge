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
