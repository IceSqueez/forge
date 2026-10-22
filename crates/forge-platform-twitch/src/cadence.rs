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
