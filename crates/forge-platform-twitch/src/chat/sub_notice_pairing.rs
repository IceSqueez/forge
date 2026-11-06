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

#[cfg(test)]
mod tests {
    use super::*;

    const NANOSECOND: Duration = Duration::from_nanos(1);

    fn sub_of(user: &str) -> SubNoticeKey {
        SubNoticeKey::new(SubNoticeKind::Sub, user)
    }

    fn resub_of(user: &str) -> SubNoticeKey {
        SubNoticeKey::new(SubNoticeKind::Resub, user)
    }

    fn released(held: &'static str, is_prime: bool) -> ReleasedSub<&'static str> {
        ReleasedSub { held, is_prime }
    }

    #[test]
    fn a_waiting_sub_is_released_by_its_notice_carrying_the_notice_prime_flag() {
        for is_prime in [true, false] {
            let start = Instant::now();
            let mut pairing = SubNoticePairing::default();
            assert_eq!(
                pairing.wait_evicting_oldest(sub_of("1"), "sub", start),
                None
            );

            let paired =
                pairing.offer_notice(sub_of("1"), is_prime, start + Duration::from_secs(1));

            assert_eq!(
                paired,
                Some(released("sub", is_prime)),
                "is_prime {is_prime}"
            );
        }
    }

    #[test]
    fn a_notice_that_arrives_first_is_claimed_by_the_later_sub() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::<&str>::default();
        assert_eq!(pairing.offer_notice(sub_of("1"), true, start), None);

        assert_eq!(
            pairing.take_notice(&sub_of("1"), start + Duration::from_secs(1)),
            Some(true)
        );
    }

    #[test]
    fn a_claimed_notice_pairs_only_once() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::<&str>::default();
        pairing.offer_notice(sub_of("1"), true, start);
        pairing.take_notice(&sub_of("1"), start);

        assert_eq!(pairing.take_notice(&sub_of("1"), start), None);
    }

    #[test]
    fn a_waiting_sub_is_not_released_before_its_wait_ends() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "sub", start);

        assert_eq!(
            pairing.release_expired(start + SUB_NOTICE_WAIT - NANOSECOND),
            Vec::new()
        );
    }

    #[test]
    fn a_waiting_sub_is_released_without_prime_exactly_when_its_wait_ends() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "sub", start);

        assert_eq!(
            pairing.release_expired(start + SUB_NOTICE_WAIT),
            vec![released("sub", false)]
        );
    }

    #[test]
    fn the_next_deadline_is_the_oldest_waiting_sub_wait_end() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        assert_eq!(pairing.next_deadline(), None);
        pairing.wait_evicting_oldest(sub_of("1"), "first", start);
        pairing.wait_evicting_oldest(sub_of("2"), "second", start + Duration::from_secs(1));

        assert_eq!(pairing.next_deadline(), Some(start + SUB_NOTICE_WAIT));
    }

    #[test]
    fn expiry_releases_only_the_subs_whose_wait_has_ended() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "first", start);
        pairing.wait_evicting_oldest(sub_of("2"), "second", start + Duration::from_secs(1));

        assert_eq!(
            pairing.release_expired(start + SUB_NOTICE_WAIT),
            vec![released("first", false)]
        );
    }

    #[test]
    fn a_notice_never_pairs_with_the_other_sub_kind_of_the_same_user() {
        for (waiting, notice) in [(sub_of("1"), resub_of("1")), (resub_of("1"), sub_of("1"))] {
            let start = Instant::now();
            let mut pairing = SubNoticePairing::default();
            pairing.wait_evicting_oldest(waiting.clone(), "held", start);

            assert_eq!(
                pairing.offer_notice(notice.clone(), true, start),
                None,
                "{notice:?} paired with {waiting:?}"
            );
        }
    }

    #[test]
    fn a_buffered_notice_is_never_claimed_by_the_other_sub_kind_of_the_same_user() {
        for (notice, claimant) in [(sub_of("1"), resub_of("1")), (resub_of("1"), sub_of("1"))] {
            let start = Instant::now();
            let mut pairing = SubNoticePairing::<&str>::default();
            pairing.offer_notice(notice.clone(), true, start);

            assert_eq!(
                pairing.take_notice(&claimant, start),
                None,
                "{claimant:?} claimed {notice:?}"
            );
        }
    }

    #[test]
    fn a_sub_at_the_waiting_limit_evicts_nothing() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        for user in 1..WAITING_SUB_LIMIT {
            pairing.wait_evicting_oldest(sub_of(&user.to_string()), user, start);
        }

        assert_eq!(
            pairing.wait_evicting_oldest(sub_of("last"), WAITING_SUB_LIMIT, start),
            None
        );
    }

    #[test]
    fn a_sub_over_the_waiting_limit_releases_the_oldest_without_prime() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        for user in 0..WAITING_SUB_LIMIT {
            pairing.wait_evicting_oldest(sub_of(&user.to_string()), user, start);
        }

        assert_eq!(
            pairing.wait_evicting_oldest(sub_of("over"), WAITING_SUB_LIMIT, start),
            Some(ReleasedSub {
                held: 0,
                is_prime: false
            })
        );
    }

    #[test]
    fn a_notice_at_exactly_its_max_age_still_pairs() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::<&str>::default();
        pairing.offer_notice(sub_of("1"), true, start);

        assert_eq!(
            pairing.take_notice(&sub_of("1"), start + UNCLAIMED_NOTICE_MAX_AGE),
            Some(true)
        );
    }

    #[test]
    fn a_notice_past_its_max_age_does_not_pair() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::<&str>::default();
        pairing.offer_notice(sub_of("1"), true, start);

        assert_eq!(
            pairing.take_notice(&sub_of("1"), start + UNCLAIMED_NOTICE_MAX_AGE + NANOSECOND),
            None
        );
    }

    #[test]
    fn a_notice_over_the_unclaimed_limit_forgets_the_oldest_notice() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::<&str>::default();
        for user in 0..=UNCLAIMED_NOTICE_LIMIT {
            pairing.offer_notice(sub_of(&user.to_string()), true, start);
        }

        assert_eq!(
            (
                pairing.take_notice(&sub_of("0"), start),
                pairing.take_notice(&sub_of("1"), start)
            ),
            (None, Some(true))
        );
    }

    #[test]
    fn releasing_all_flushes_every_waiting_sub_without_prime_in_arrival_order() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "first", start);
        pairing.wait_evicting_oldest(resub_of("2"), "second", start);

        assert_eq!(
            pairing.release_all(),
            vec![released("first", false), released("second", false)]
        );
    }

    #[test]
    fn a_paired_sub_is_never_released_again_by_expiry_or_flush() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "paired", start);
        pairing.offer_notice(sub_of("1"), true, start);

        assert_eq!(
            (
                pairing.release_expired(start + SUB_NOTICE_WAIT),
                pairing.release_all()
            ),
            (Vec::new(), Vec::new())
        );
    }

    #[test]
    fn an_expired_sub_is_never_released_again_by_a_flush() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "expired", start);
        pairing.release_expired(start + SUB_NOTICE_WAIT);

        assert_eq!(pairing.release_all(), Vec::new());
    }

    #[test]
    fn a_flushed_sub_is_never_released_again_by_expiry() {
        let start = Instant::now();
        let mut pairing = SubNoticePairing::default();
        pairing.wait_evicting_oldest(sub_of("1"), "flushed", start);
        pairing.release_all();

        assert_eq!(pairing.release_expired(start + SUB_NOTICE_WAIT), Vec::new());
    }
}
