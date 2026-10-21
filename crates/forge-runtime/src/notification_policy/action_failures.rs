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
