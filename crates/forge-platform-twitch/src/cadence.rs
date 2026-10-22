use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

pub(crate) const ANNOUNCEMENT_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const SHOUTOUT_CHANNEL_INTERVAL: Duration = Duration::from_secs(2 * 60);
pub(crate) const SHOUTOUT_TARGET_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, PartialEq, Eq, Hash)]
enum CadenceKey {
    ShoutoutChannel,
    ShoutoutTarget(String),
}

#[derive(Default)]
struct AnnouncementState {
    committed: Option<Instant>,
    pending: Vec<Instant>,
}

#[derive(Default)]
pub(crate) struct CadenceLedger {
    last: Mutex<HashMap<CadenceKey, Instant>>,
    announcements: Mutex<AnnouncementState>,
}

pub(crate) struct AnnouncementReservation<'a> {
    ledger: &'a CadenceLedger,
    slot: Instant,
    wait: Duration,
    settled: bool,
}

impl AnnouncementReservation<'_> {
    pub(crate) fn wait(&self) -> Duration {
        self.wait
    }

    pub(crate) fn commit(mut self) {
        self.settled = true;
        let mut state = self.ledger.state();
        state.pending.retain(|slot| *slot != self.slot);
        state.committed = state.committed.max(Some(self.slot));
    }
}

impl Drop for AnnouncementReservation<'_> {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let mut state = self.ledger.state();
        state.pending.retain(|slot| *slot != self.slot);
    }
}

impl CadenceLedger {
    fn state(&self) -> std::sync::MutexGuard<'_, AnnouncementState> {
        self.announcements.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn reserve_announcement(&self) -> AnnouncementReservation<'_> {
        let now = Instant::now();
        let mut state = self.state();
        let slot = state
            .pending
            .iter()
            .copied()
            .chain(state.committed)
            .max()
            .map(|latest| latest + ANNOUNCEMENT_INTERVAL)
            .filter(|next| *next > now)
            .unwrap_or(now);
        state.pending.push(slot);
        AnnouncementReservation {
            ledger: self,
            slot,
            wait: slot.saturating_duration_since(now),
            settled: false,
        }
    }

    pub(crate) fn shoutout_cooldown(&self, target_keys: &[&str]) -> Option<Duration> {
        let now = Instant::now();
        let last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        let remaining = |key: CadenceKey, interval: Duration| {
            last.get(&key)
                .and_then(|previous| (*previous + interval).checked_duration_since(now))
                .filter(|remaining| !remaining.is_zero())
        };
        let channel = remaining(CadenceKey::ShoutoutChannel, SHOUTOUT_CHANNEL_INTERVAL);
        let targets = target_keys.iter().filter_map(|key| {
            remaining(
                CadenceKey::ShoutoutTarget((*key).to_owned()),
                SHOUTOUT_TARGET_INTERVAL,
            )
        });
        channel.into_iter().chain(targets).max()
    }

    pub(crate) fn record_shoutout(&self, target_keys: &[&str]) {
        let now = Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        last.retain(|key, at| match key {
            CadenceKey::ShoutoutTarget(..) => at.elapsed() < SHOUTOUT_TARGET_INTERVAL,
            _ => true,
        });
        last.insert(CadenceKey::ShoutoutChannel, now);
        for key in target_keys {
            last.insert(CadenceKey::ShoutoutTarget((*key).to_owned()), now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[tokio::test(start_paused = true)]
    async fn committed_announcements_are_spaced_exactly_by_the_interval() {
        let ledger = CadenceLedger::default();

        let first = ledger.reserve_announcement();
        let first_wait = first.wait();
        first.commit();
        let second = ledger.reserve_announcement();
        let second_wait = second.wait();
        second.commit();
        tokio::time::advance(SECOND).await;
        let third_wait = ledger.reserve_announcement().wait();

        assert_eq!(first_wait, Duration::ZERO);
        assert_eq!(second_wait, ANNOUNCEMENT_INTERVAL);
        assert_eq!(third_wait, 2 * ANNOUNCEMENT_INTERVAL - SECOND);
    }

    #[tokio::test(start_paused = true)]
    async fn an_announcement_after_the_interval_has_passed_goes_out_at_once() {
        let ledger = CadenceLedger::default();
        ledger.reserve_announcement().commit();

        tokio::time::advance(ANNOUNCEMENT_INTERVAL).await;

        assert_eq!(ledger.reserve_announcement().wait(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn an_uncommitted_reservation_gives_its_slot_back() {
        let ledger = CadenceLedger::default();
        drop(ledger.reserve_announcement());
        let after_first_released = ledger.reserve_announcement();
        let after_first_released_wait = after_first_released.wait();
        after_first_released.commit();

        drop(ledger.reserve_announcement());
        let after_second_released = ledger.reserve_announcement().wait();

        assert_eq!(after_first_released_wait, Duration::ZERO);
        assert_eq!(after_second_released, ANNOUNCEMENT_INTERVAL);
    }

    #[tokio::test(start_paused = true)]
    async fn releasing_an_earlier_reservation_never_frees_a_later_ones_slot() {
        let ledger = CadenceLedger::default();
        ledger.reserve_announcement().commit();
        let earlier = ledger.reserve_announcement();
        let later = ledger.reserve_announcement();

        drop(earlier);
        let next_wait = ledger.reserve_announcement().wait();

        assert_eq!(later.wait(), 2 * ANNOUNCEMENT_INTERVAL);
        assert_eq!(next_wait, 3 * ANNOUNCEMENT_INTERVAL);
    }

    #[tokio::test(start_paused = true)]
    async fn the_channel_cooldown_blocks_every_target_for_two_minutes() {
        let ledger = CadenceLedger::default();
        ledger.record_shoutout(&["id:1"]);

        let at_start = ledger.shoutout_cooldown(&["id:2"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL - SECOND).await;
        let one_second_left = ledger.shoutout_cooldown(&["id:2"]);
        tokio::time::advance(2 * SECOND).await;
        let after_window = ledger.shoutout_cooldown(&["id:2"]);

        assert_eq!(at_start, Some(SHOUTOUT_CHANNEL_INTERVAL));
        assert_eq!(one_second_left, Some(SECOND));
        assert_eq!(after_window, None);
    }

    #[tokio::test(start_paused = true)]
    async fn the_target_cooldown_blocks_the_same_target_for_an_hour() {
        let ledger = CadenceLedger::default();
        ledger.record_shoutout(&["id:1"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL + SECOND).await;

        let same_target = ledger.shoutout_cooldown(&["id:1"]);
        tokio::time::advance(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL).await;
        let after_window = ledger.shoutout_cooldown(&["id:1"]);

        assert_eq!(
            same_target,
            Some(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL - SECOND)
        );
        assert_eq!(after_window, None);
    }

    #[tokio::test(start_paused = true)]
    async fn every_recorded_key_of_a_target_blocks_it() {
        let ledger = CadenceLedger::default();
        ledger.record_shoutout(&["login:target", "id:555"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL + SECOND).await;

        assert!(ledger.shoutout_cooldown(&["login:target"]).is_some());
        assert!(ledger.shoutout_cooldown(&["id:555"]).is_some());
        assert!(
            ledger
                .shoutout_cooldown(&["id:777", "login:other"])
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_every_queued_announcement_frees_the_queue_entirely() {
        let ledger = CadenceLedger::default();
        let second = ledger.reserve_announcement();
        let third = ledger.reserve_announcement();
        let third_wait = third.wait();

        drop(second);
        drop(third);
        let next_wait = ledger.reserve_announcement().wait();

        assert_eq!(third_wait, ANNOUNCEMENT_INTERVAL);
        assert_eq!(next_wait, Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_reservations_leave_only_the_committed_send_to_space_from() {
        let ledger = CadenceLedger::default();
        ledger.reserve_announcement().commit();
        drop(ledger.reserve_announcement());
        drop(ledger.reserve_announcement());

        let next_wait = ledger.reserve_announcement().wait();

        assert_eq!(next_wait, ANNOUNCEMENT_INTERVAL);
    }

    #[tokio::test(start_paused = true)]
    async fn a_later_reservation_committed_after_an_earlier_cancel_spaces_the_next_from_itself() {
        let ledger = CadenceLedger::default();
        ledger.reserve_announcement().commit();
        let earlier = ledger.reserve_announcement();
        let later = ledger.reserve_announcement();
        drop(earlier);

        later.commit();
        let next_wait = ledger.reserve_announcement().wait();

        assert_eq!(next_wait, 3 * ANNOUNCEMENT_INTERVAL);
    }

    #[tokio::test(start_paused = true)]
    async fn shoutout_cooldowns_end_exactly_at_their_window() {
        let ledger = CadenceLedger::default();
        ledger.record_shoutout(&["id:1"]);

        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL).await;
        let channel_at_boundary = ledger.shoutout_cooldown(&["id:2"]);
        tokio::time::advance(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL).await;
        let target_at_boundary = ledger.shoutout_cooldown(&["id:1"]);

        assert_eq!(channel_at_boundary, None);
        assert_eq!(target_at_boundary, None);
    }
}
