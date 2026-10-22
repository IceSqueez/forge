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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    enum Poll {
        Ok,
        Fail,
    }

    fn reports_after(polls: &[Poll]) -> Vec<PollLinkHealth> {
        let reports = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&reports);
        let mut tracker = PollHealthTracker::new(Some(Arc::new(move |health| {
            sink.lock().unwrap().push(health);
        })));
        for poll in polls {
            match poll {
                Poll::Ok => tracker.succeeded(),
                Poll::Fail => tracker.failed(),
            }
        }
        reports.lock().unwrap().clone()
    }

    fn failures(count: u32) -> Vec<Poll> {
        (0..count).map(|_| Poll::Fail).collect()
    }

    #[test]
    fn loss_is_reported_exactly_at_the_consecutive_failure_threshold() {
        for (count, expected) in [
            (CONSECUTIVE_POLL_FAILURES_FOR_LOSS - 1, vec![]),
            (
                CONSECUTIVE_POLL_FAILURES_FOR_LOSS,
                vec![PollLinkHealth::Lost],
            ),
        ] {
            assert_eq!(
                reports_after(&failures(count)),
                expected,
                "{count} failures"
            );
        }
    }

    #[test]
    fn failures_past_the_threshold_report_the_loss_once() {
        let reports = reports_after(&failures(CONSECUTIVE_POLL_FAILURES_FOR_LOSS * 2));
        assert_eq!(reports, [PollLinkHealth::Lost]);
    }

    #[test]
    fn success_before_the_threshold_restarts_the_failure_count() {
        let mut polls = failures(CONSECUTIVE_POLL_FAILURES_FOR_LOSS - 1);
        polls.push(Poll::Ok);
        polls.extend(failures(CONSECUTIVE_POLL_FAILURES_FOR_LOSS - 1));
        assert!(reports_after(&polls).is_empty());
    }

    #[test]
    fn first_success_after_a_loss_reports_recovery_once() {
        let mut polls = failures(CONSECUTIVE_POLL_FAILURES_FOR_LOSS);
        polls.extend([Poll::Ok, Poll::Ok]);
        assert_eq!(
            reports_after(&polls),
            [PollLinkHealth::Lost, PollLinkHealth::Healthy]
        );
    }

    #[test]
    fn successes_on_a_healthy_link_report_nothing() {
        assert!(reports_after(&[Poll::Ok, Poll::Ok]).is_empty());
    }

    #[test]
    fn link_recovered_from_a_loss_needs_a_full_new_streak_to_be_lost_again() {
        let mut polls = failures(CONSECUTIVE_POLL_FAILURES_FOR_LOSS);
        polls.push(Poll::Ok);
        polls.extend(failures(CONSECUTIVE_POLL_FAILURES_FOR_LOSS - 1));
        assert_eq!(
            reports_after(&polls),
            [PollLinkHealth::Lost, PollLinkHealth::Healthy]
        );
    }
}
