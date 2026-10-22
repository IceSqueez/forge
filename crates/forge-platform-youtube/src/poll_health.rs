use std::sync::Arc;

pub(crate) const CONSECUTIVE_POLL_FAILURES_FOR_LOSS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PollLinkHealth {
    Healthy,
    Lost,
}

pub(crate) type PollHealthReporter = Arc<dyn Fn(PollLinkHealth) + Send + Sync>;

pub(crate) struct PollHealthTracker {
    reporter: Option<PollHealthReporter>,
    consecutive_failures: u32,
    lost: bool,
}

impl PollHealthTracker {
    pub(crate) fn new(reporter: Option<PollHealthReporter>) -> Self {
        Self {
            reporter,
            consecutive_failures: 0,
            lost: false,
        }
    }

    pub(crate) fn succeeded(&mut self) {
        self.consecutive_failures = 0;
        if self.lost {
            self.lost = false;
            self.report(PollLinkHealth::Healthy);
        }
    }

    pub(crate) fn failed(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if !self.lost && self.consecutive_failures >= CONSECUTIVE_POLL_FAILURES_FOR_LOSS {
            self.lost = true;
            self.report(PollLinkHealth::Lost);
        }
    }

    fn report(&self, health: PollLinkHealth) {
        if let Some(reporter) = &self.reporter {
            reporter(health);
        }
    }
}
