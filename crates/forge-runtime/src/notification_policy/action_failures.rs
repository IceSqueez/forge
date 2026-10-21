use std::collections::HashMap;
use std::time::Duration;

use forge_types::ActionId;
use tokio::time::Instant;

use super::intent::{ActionFailureNotice, NotificationIntent};

pub const ACTION_FAILURE_REARM: Duration = Duration::from_secs(30 * 60);
pub const ACTION_NOTICE_WINDOW: Duration = Duration::from_secs(60);
pub const MAX_ACTION_NOTICES_PER_WINDOW: u32 = 3;

struct NoticeWindow {
    opened_at: Instant,
    shown: u32,
    folded: u32,
}

impl NoticeWindow {
    fn opened(now: Instant) -> Self {
        Self {
            opened_at: now,
            shown: 1,
            folded: 0,
        }
    }

    fn closes_at(&self) -> Instant {
        self.opened_at + ACTION_NOTICE_WINDOW
    }
}

#[derive(Default)]
pub(super) struct ActionFailureGate {
    silenced_since: HashMap<ActionId, Instant>,
    window: Option<NoticeWindow>,
}

impl ActionFailureGate {
    pub(super) fn succeeded(&mut self, action_id: &ActionId) {
        self.silenced_since.remove(action_id);
    }

    pub(super) fn failed(
        &mut self,
        notice: ActionFailureNotice,
        now: Instant,
        out: &mut Vec<NotificationIntent>,
    ) {
        self.flush_due(now, out);
        let still_silenced = self
            .silenced_since
            .get(&notice.action_id)
            .is_some_and(|since| now.saturating_duration_since(*since) < ACTION_FAILURE_REARM);
        if still_silenced {
            return;
        }
        self.silenced_since
            .retain(|_, since| now.saturating_duration_since(*since) < ACTION_FAILURE_REARM);
        self.silenced_since.insert(notice.action_id, now);
        if self.admit(now) {
            out.push(NotificationIntent::ActionFailed(notice));
        }
    }

    pub(super) fn flush_due(&mut self, now: Instant, out: &mut Vec<NotificationIntent>) {
        let Some(window) = &self.window else {
            return;
        };
        if now < window.closes_at() {
            return;
        }
        let folded = window.folded;
        if folded == 0 {
            self.window = None;
            return;
        }
        out.push(NotificationIntent::ActionFailuresFolded { count: folded });
        self.window = Some(NoticeWindow::opened(now));
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.window
            .as_ref()
            .filter(|window| window.folded > 0)
            .map(NoticeWindow::closes_at)
    }

    fn admit(&mut self, now: Instant) -> bool {
        match &mut self.window {
            Some(window) if now < window.closes_at() => {
                if window.shown < MAX_ACTION_NOTICES_PER_WINDOW {
                    window.shown += 1;
                    true
                } else {
                    window.folded += 1;
                    false
                }
            }
            _ => {
                self.window = Some(NoticeWindow::opened(now));
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn notice(action_id: ActionId) -> ActionFailureNotice {
        ActionFailureNotice {
            action_id,
            action_name: "action".to_owned(),
            reason: None,
        }
    }

    fn fail(
        gate: &mut ActionFailureGate,
        action_id: ActionId,
        at: Instant,
    ) -> Vec<NotificationIntent> {
        let mut out = Vec::new();
        gate.failed(notice(action_id), at, &mut out);
        out
    }

    fn flush(gate: &mut ActionFailureGate, at: Instant) -> Vec<NotificationIntent> {
        let mut out = Vec::new();
        gate.flush_due(at, &mut out);
        out
    }

    fn fail_distinct(
        gate: &mut ActionFailureGate,
        count: usize,
        at: Instant,
    ) -> Vec<NotificationIntent> {
        (0..count)
            .flat_map(|_| fail(gate, ActionId::new(), at))
            .collect()
    }

    fn shown(intents: &[NotificationIntent]) -> usize {
        intents
            .iter()
            .filter(|intent| matches!(intent, NotificationIntent::ActionFailed(_)))
            .count()
    }

    #[test]
    fn repeated_failures_of_one_action_notify_only_the_first() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        let action = ActionId::new();
        let intents: Vec<_> = [Duration::ZERO, 10 * SECOND, ACTION_FAILURE_REARM - SECOND]
            .into_iter()
            .flat_map(|offset| fail(&mut gate, action, t0 + offset))
            .collect();
        assert_eq!(
            intents,
            vec![NotificationIntent::ActionFailed(notice(action))]
        );
    }

    #[test]
    fn silenced_action_rearms_exactly_when_the_rearm_window_elapses() {
        for (offset, notifies_again) in [
            (ACTION_FAILURE_REARM - SECOND, false),
            (ACTION_FAILURE_REARM, true),
        ] {
            let t0 = Instant::now();
            let mut gate = ActionFailureGate::default();
            let action = ActionId::new();
            fail(&mut gate, action, t0);
            let again = fail(&mut gate, action, t0 + offset);
            assert_eq!(shown(&again) == 1, notifies_again, "offset {offset:?}");
        }
    }

    #[test]
    fn success_rearms_the_action_for_its_next_failure() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        let action = ActionId::new();
        fail(&mut gate, action, t0);
        gate.succeeded(&action);
        assert_eq!(shown(&fail(&mut gate, action, t0 + SECOND)), 1);
    }

    #[test]
    fn success_of_another_action_keeps_the_failing_action_silenced() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        let action = ActionId::new();
        fail(&mut gate, action, t0);
        gate.succeeded(&ActionId::new());
        assert!(fail(&mut gate, action, t0 + SECOND).is_empty());
    }

    #[test]
    fn burst_cap_shows_three_notices_and_folds_the_rest_at_window_close() {
        for (failures, folded) in [(3, None), (4, Some(1)), (7, Some(4))] {
            let t0 = Instant::now();
            let mut gate = ActionFailureGate::default();
            let intents = fail_distinct(&mut gate, failures, t0);
            assert_eq!(shown(&intents), 3, "{failures} failures");
            let summary = flush(&mut gate, t0 + ACTION_NOTICE_WINDOW);
            let expected: Vec<_> = folded
                .map(|count| NotificationIntent::ActionFailuresFolded { count })
                .into_iter()
                .collect();
            assert_eq!(summary, expected, "{failures} failures");
        }
    }

    #[test]
    fn deadline_is_set_only_while_failures_are_folded() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        fail_distinct(&mut gate, 3, t0);
        assert_eq!(gate.deadline(), None);
        fail_distinct(&mut gate, 1, t0);
        assert_eq!(gate.deadline(), Some(t0 + ACTION_NOTICE_WINDOW));
    }

    #[test]
    fn folded_summary_is_held_until_the_window_closes() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        fail_distinct(&mut gate, 4, t0);
        let early = flush(&mut gate, t0 + ACTION_NOTICE_WINDOW - SECOND);
        assert!(early.is_empty());
        assert_eq!(gate.deadline(), Some(t0 + ACTION_NOTICE_WINDOW));
    }

    #[test]
    fn failure_at_window_close_opens_a_fresh_window() {
        for (offset, notifies) in [
            (ACTION_NOTICE_WINDOW - SECOND, false),
            (ACTION_NOTICE_WINDOW, true),
        ] {
            let t0 = Instant::now();
            let mut gate = ActionFailureGate::default();
            fail_distinct(&mut gate, 3, t0);
            let next = fail_distinct(&mut gate, 1, t0 + offset);
            assert_eq!(shown(&next) == 1, notifies, "offset {offset:?}");
        }
    }

    #[test]
    fn folded_summary_counts_against_the_next_window_cap() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        fail_distinct(&mut gate, 4, t0);
        let close = t0 + ACTION_NOTICE_WINDOW;
        flush(&mut gate, close);
        let next = fail_distinct(&mut gate, 3, close + SECOND);
        assert_eq!(shown(&next), 2);
    }

    #[test]
    fn folded_action_stays_silenced_after_the_summary() {
        let t0 = Instant::now();
        let mut gate = ActionFailureGate::default();
        fail_distinct(&mut gate, 3, t0);
        let folded_action = ActionId::new();
        fail(&mut gate, folded_action, t0);
        let later = fail(&mut gate, folded_action, t0 + ACTION_NOTICE_WINDOW + SECOND);
        assert_eq!(
            later,
            vec![NotificationIntent::ActionFailuresFolded { count: 1 }]
        );
    }
}
