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

    pub(crate) fn reserve_shoutout(
        &self,
        target_keys: &[&str],
    ) -> Result<ShoutoutReservation<'_>, Duration> {
        let mut reservation = ShoutoutReservation {
            ledger: self,
            undo: Vec::new(),
            settled: false,
        };
        reservation.extend(target_keys)?;
        Ok(reservation)
    }
}

pub(crate) struct ShoutoutReservation<'a> {
    ledger: &'a CadenceLedger,
    undo: Vec<(CadenceKey, Option<Instant>)>,
    settled: bool,
}

impl ShoutoutReservation<'_> {
    pub(crate) fn extend(&mut self, target_keys: &[&str]) -> Result<(), Duration> {
        let now = Instant::now();
        let mut last = self.ledger.last.lock().unwrap_or_else(|p| p.into_inner());
        let held = |key: &CadenceKey| self.undo.iter().any(|(reserved, _)| reserved == key);
        let remaining = |last: &HashMap<CadenceKey, Instant>, key: CadenceKey, interval| {
            if held(&key) {
                return None;
            }
            last.get(&key)
                .and_then(|previous| (*previous + interval).checked_duration_since(now))
                .filter(|remaining| !remaining.is_zero())
        };
        let channel = remaining(
            &last,
            CadenceKey::ShoutoutChannel,
            SHOUTOUT_CHANNEL_INTERVAL,
        );
        let targets = target_keys.iter().filter_map(|key| {
            remaining(
                &last,
                CadenceKey::ShoutoutTarget((*key).to_owned()),
                SHOUTOUT_TARGET_INTERVAL,
            )
        });
        if let Some(blocked) = channel.into_iter().chain(targets).max() {
            return Err(blocked);
        }
        last.retain(|key, at| match key {
            CadenceKey::ShoutoutTarget(..) => at.elapsed() < SHOUTOUT_TARGET_INTERVAL,
            _ => true,
        });
        let keys = std::iter::once(CadenceKey::ShoutoutChannel).chain(
            target_keys
                .iter()
                .map(|key| CadenceKey::ShoutoutTarget((*key).to_owned())),
        );
        let fresh: Vec<CadenceKey> = keys.filter(|key| !held(key)).collect();
        for key in fresh {
            let previous = last.insert(key.clone(), now);
            self.undo.push((key, previous));
        }
        Ok(())
    }

    pub(crate) fn commit(mut self) {
        self.settled = true;
    }
}

impl Drop for ShoutoutReservation<'_> {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let mut last = self.ledger.last.lock().unwrap_or_else(|p| p.into_inner());
        for (key, previous) in self.undo.drain(..).rev() {
            match previous {
                Some(at) => last.insert(key, at),
                None => last.remove(&key),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn record_shoutout(ledger: &CadenceLedger, target_keys: &[&str]) {
        match ledger.reserve_shoutout(target_keys) {
            Ok(reservation) => reservation.commit(),
            Err(remaining) => panic!("expected a free slot, {remaining:?} left"),
        }
    }

    fn shoutout_cooldown(ledger: &CadenceLedger, target_keys: &[&str]) -> Option<Duration> {
        ledger.reserve_shoutout(target_keys).err()
    }

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
        record_shoutout(&ledger, &["id:1"]);

        let at_start = shoutout_cooldown(&ledger, &["id:2"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL - SECOND).await;
        let one_second_left = shoutout_cooldown(&ledger, &["id:2"]);
        tokio::time::advance(2 * SECOND).await;
        let after_window = shoutout_cooldown(&ledger, &["id:2"]);

        assert_eq!(at_start, Some(SHOUTOUT_CHANNEL_INTERVAL));
        assert_eq!(one_second_left, Some(SECOND));
        assert_eq!(after_window, None);
    }

    #[tokio::test(start_paused = true)]
    async fn the_target_cooldown_blocks_the_same_target_for_an_hour() {
        let ledger = CadenceLedger::default();
        record_shoutout(&ledger, &["id:1"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL + SECOND).await;

        let same_target = shoutout_cooldown(&ledger, &["id:1"]);
        tokio::time::advance(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL).await;
        let after_window = shoutout_cooldown(&ledger, &["id:1"]);

        assert_eq!(
            same_target,
            Some(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL - SECOND)
        );
        assert_eq!(after_window, None);
    }

    #[tokio::test(start_paused = true)]
    async fn every_recorded_key_of_a_target_blocks_it() {
        let ledger = CadenceLedger::default();
        record_shoutout(&ledger, &["login:target", "id:555"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL + SECOND).await;

        assert!(shoutout_cooldown(&ledger, &["login:target"]).is_some());
        assert!(shoutout_cooldown(&ledger, &["id:555"]).is_some());
        assert!(shoutout_cooldown(&ledger, &["id:777", "login:other"]).is_none());
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
        record_shoutout(&ledger, &["id:1"]);

        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL).await;
        let channel_at_boundary = shoutout_cooldown(&ledger, &["id:2"]);
        tokio::time::advance(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL).await;
        let target_at_boundary = shoutout_cooldown(&ledger, &["id:1"]);

        assert_eq!(channel_at_boundary, None);
        assert_eq!(target_at_boundary, None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_reservation_extends_to_the_resolved_id_of_its_own_target() {
        let ledger = CadenceLedger::default();
        let Ok(mut reservation) = ledger.reserve_shoutout(&["login:target"]) else {
            panic!("the first reservation must find the channel free");
        };

        assert_eq!(reservation.extend(&["id:555"]), Ok(()));
    }

    #[tokio::test(start_paused = true)]
    async fn a_held_reservation_blocks_a_concurrent_shoutout() {
        let ledger = CadenceLedger::default();
        let Ok(_held) = ledger.reserve_shoutout(&["id:1"]) else {
            panic!("the first reservation must find the channel free");
        };

        assert_eq!(
            shoutout_cooldown(&ledger, &["id:2"]),
            Some(SHOUTOUT_CHANNEL_INTERVAL)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_reservation_gives_back_every_key_it_took() {
        let ledger = CadenceLedger::default();
        let Ok(mut reservation) = ledger.reserve_shoutout(&["login:target"]) else {
            panic!("the first reservation must find the channel free");
        };
        assert_eq!(reservation.extend(&["id:555"]), Ok(()));

        drop(reservation);

        assert_eq!(
            shoutout_cooldown(&ledger, &["login:target", "id:555"]),
            None
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_extension_releases_the_channel_once_the_reservation_drops() {
        let ledger = CadenceLedger::default();
        record_shoutout(&ledger, &["id:1"]);
        tokio::time::advance(SHOUTOUT_CHANNEL_INTERVAL).await;
        let Ok(mut reservation) = ledger.reserve_shoutout(&["login:alias"]) else {
            panic!("the channel window has passed");
        };

        let extended = reservation.extend(&["id:1"]);
        drop(reservation);

        assert_eq!(
            extended,
            Err(SHOUTOUT_TARGET_INTERVAL - SHOUTOUT_CHANNEL_INTERVAL)
        );
        assert_eq!(shoutout_cooldown(&ledger, &["id:2"]), None);
    }
}
