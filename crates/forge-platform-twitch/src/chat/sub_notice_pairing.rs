use std::collections::VecDeque;
use tokio::time::{Duration, Instant};

const SUB_NOTICE_WAIT: Duration = Duration::from_secs(2);
const UNCLAIMED_NOTICE_MAX_AGE: Duration = Duration::from_secs(2);
const WAITING_SUB_LIMIT: usize = 64;
const UNCLAIMED_NOTICE_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubNoticeKind {
    Sub,
    Resub,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SubNoticeKey {
    kind: SubNoticeKind,
    chatter_id: String,
}

impl SubNoticeKey {
    pub(super) fn new(kind: SubNoticeKind, chatter_id: impl Into<String>) -> Self {
        Self {
            kind,
            chatter_id: chatter_id.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReleasedSub<T> {
    pub(super) held: T,
    pub(super) is_prime: bool,
}

impl<T> ReleasedSub<T> {
    pub(super) fn without_notice(held: T) -> Self {
        Self {
            held,
            is_prime: false,
        }
    }
}

struct WaitingSub<T> {
    key: SubNoticeKey,
    deadline: Instant,
    held: T,
}

struct UnclaimedNotice {
    key: SubNoticeKey,
    is_prime: bool,
    received_at: Instant,
}

pub(super) struct SubNoticePairing<T> {
    waiting: VecDeque<WaitingSub<T>>,
    unclaimed: VecDeque<UnclaimedNotice>,
}

impl<T> Default for SubNoticePairing<T> {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
            unclaimed: VecDeque::new(),
        }
    }
}

impl<T> SubNoticePairing<T> {
    pub(super) fn take_notice(&mut self, key: &SubNoticeKey, now: Instant) -> Option<bool> {
        self.forget_stale_notices(now);
        let index = self
            .unclaimed
            .iter()
            .position(|notice| notice.key == *key)?;
        self.unclaimed.remove(index).map(|notice| notice.is_prime)
    }

    pub(super) fn wait_evicting_oldest(
        &mut self,
        key: SubNoticeKey,
        held: T,
        now: Instant,
    ) -> Option<ReleasedSub<T>> {
        let evicted = if self.waiting.len() >= WAITING_SUB_LIMIT {
            self.waiting.pop_front().map(WaitingSub::without_notice)
        } else {
            None
        };
        self.waiting.push_back(WaitingSub {
            key,
            deadline: now + SUB_NOTICE_WAIT,
            held,
        });
        evicted
    }

    pub(super) fn offer_notice(
        &mut self,
        key: SubNoticeKey,
        is_prime: bool,
        now: Instant,
    ) -> Option<ReleasedSub<T>> {
        if let Some(index) = self.waiting.iter().position(|waiting| waiting.key == key) {
            return self.waiting.remove(index).map(|waiting| ReleasedSub {
                held: waiting.held,
                is_prime,
            });
        }
        self.forget_stale_notices(now);
        if self.unclaimed.len() >= UNCLAIMED_NOTICE_LIMIT {
            self.unclaimed.pop_front();
        }
        self.unclaimed.push_back(UnclaimedNotice {
            key,
            is_prime,
            received_at: now,
        });
        None
    }

    pub(super) fn release_expired(&mut self, now: Instant) -> Vec<ReleasedSub<T>> {
        let mut released = Vec::new();
        while let Some(waiting) = self.waiting.pop_front_if(|waiting| waiting.deadline <= now) {
            released.push(waiting.without_notice());
        }
        released
    }

    pub(super) fn release_all(&mut self) -> Vec<ReleasedSub<T>> {
        self.waiting
            .drain(..)
            .map(WaitingSub::without_notice)
            .collect()
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.waiting.front().map(|waiting| waiting.deadline)
    }

    fn forget_stale_notices(&mut self, now: Instant) {
        self.unclaimed.retain(|notice| {
            now.saturating_duration_since(notice.received_at) <= UNCLAIMED_NOTICE_MAX_AGE
        });
    }
}

impl<T> WaitingSub<T> {
    fn without_notice(self) -> ReleasedSub<T> {
        ReleasedSub::without_notice(self.held)
    }
}
