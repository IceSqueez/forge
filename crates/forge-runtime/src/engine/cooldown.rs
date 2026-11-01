use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_types::{EventId, MonotonicClock, SystemMonotonicClock, TriggerInstanceId};

pub(crate) type CooldownKey = (TriggerInstanceId, Option<String>);

struct CooldownEntry {
    stamped_at: Instant,
    window: Duration,
    event_id: EventId,
}

impl CooldownEntry {
    fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.stamped_at) >= self.window
    }
}

pub(crate) struct CooldownMap {
    entries: HashMap<CooldownKey, CooldownEntry>,
    capacity: usize,
    clock: Arc<dyn MonotonicClock + Send + Sync>,
}

impl CooldownMap {
    pub(crate) fn new(capacity: usize) -> Self {
        Self::on_clock(capacity, Arc::new(SystemMonotonicClock))
    }

    fn on_clock(capacity: usize, clock: Arc<dyn MonotonicClock + Send + Sync>) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            clock,
        }
    }

    pub(crate) fn remaining_or_stamp(
        &mut self,
        key: CooldownKey,
        window: Duration,
        event_id: EventId,
    ) -> Option<Duration> {
        let now = self.clock.now();
        if let Some(entry) = self.entries.get(&key) {
            if entry.event_id == event_id {
                return None;
            }
            let elapsed = now.saturating_duration_since(entry.stamped_at);
            if elapsed < window {
                return Some(window - elapsed);
            }
        }

        self.entries.retain(|_, entry| !entry.is_expired(now));

        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.evict_oldest();
        }

        self.entries.insert(
            key,
            CooldownEntry {
                stamped_at: now,
                window,
                event_id,
            },
        );
        None
    }

    pub(crate) fn release(&mut self, key: &CooldownKey, event_id: EventId) {
        if self
            .entries
            .get(key)
            .is_some_and(|entry| entry.event_id == event_id)
        {
            self.entries.remove(key);
        }
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.stamped_at)
            .map(|(key, _)| key.clone());

        if let Some(key) = oldest {
            self.entries.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);
    const TICK: Duration = Duration::from_secs(1);

    struct ManualClock(std::sync::Mutex<Instant>);

    impl ManualClock {
        fn started() -> Arc<Self> {
            Arc::new(Self(std::sync::Mutex::new(Instant::now())))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    impl MonotonicClock for ManualClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }

        fn sleep(&self, duration: Duration) {
            self.advance(duration);
        }
    }

    fn map_on_manual_clock(capacity: usize) -> (CooldownMap, Arc<ManualClock>) {
        let clock = ManualClock::started();
        (CooldownMap::on_clock(capacity, clock.clone()), clock)
    }

    fn user_key(instance: TriggerInstanceId, user: &str) -> CooldownKey {
        (instance, Some(user.to_owned()))
    }

    #[test]
    fn a_later_event_inside_the_window_reports_the_remaining_time() {
        let (mut map, clock) = map_on_manual_clock(16);
        let key = user_key(TriggerInstanceId::new(), "alice");

        assert!(
            map.remaining_or_stamp(key.clone(), WINDOW, EventId::new())
                .is_none(),
            "an unstamped key must pass and stamp"
        );

        clock.advance(TICK);
        assert_eq!(
            map.remaining_or_stamp(key, WINDOW, EventId::new()),
            Some(WINDOW - TICK)
        );
    }

    #[test]
    fn a_window_holds_until_its_last_tick_and_reopens_exactly_at_its_end() {
        let (mut map, clock) = map_on_manual_clock(16);
        let key = user_key(TriggerInstanceId::new(), "alice");
        map.remaining_or_stamp(key.clone(), WINDOW, EventId::new());

        clock.advance(WINDOW - TICK);
        assert_eq!(
            map.remaining_or_stamp(key.clone(), WINDOW, EventId::new()),
            Some(TICK)
        );

        clock.advance(TICK);
        assert_eq!(map.remaining_or_stamp(key, WINDOW, EventId::new()), None);
    }

    #[test]
    fn the_stamping_event_does_not_throttle_itself() {
        let mut map = CooldownMap::new(16);
        let key = user_key(TriggerInstanceId::new(), "alice");
        let event = EventId::new();

        assert!(
            map.remaining_or_stamp(key.clone(), WINDOW, event).is_none(),
            "first evaluation stamps"
        );
        assert!(
            map.remaining_or_stamp(key, WINDOW, event).is_none(),
            "the stamping event must not throttle itself"
        );
    }

    #[test]
    fn the_capacity_cap_evicts_oldest_first_and_only_that_key_fails_open() {
        let (mut map, clock) = map_on_manual_clock(2);
        let instance = TriggerInstanceId::new();
        let (first, second, third) = (
            user_key(instance, "first"),
            user_key(instance, "second"),
            user_key(instance, "third"),
        );

        for key in [first.clone(), second.clone(), third] {
            assert!(
                map.remaining_or_stamp(key, WINDOW, EventId::new())
                    .is_none(),
                "a distinct chatter inside the window is never throttled on their first call"
            );
            clock.advance(TICK);
        }

        assert!(
            map.remaining_or_stamp(second, WINDOW, EventId::new())
                .is_some(),
            "a key that survived the cap must still throttle"
        );
        assert!(
            map.remaining_or_stamp(first, WINDOW, EventId::new())
                .is_none(),
            "the oldest key is evicted, so its throttle fails open"
        );
    }

    #[test]
    fn an_expired_entry_is_pruned_without_the_cap_being_reached() {
        let (mut map, clock) = map_on_manual_clock(1024);
        let instance = TriggerInstanceId::new();

        assert!(
            map.remaining_or_stamp(user_key(instance, "gone"), WINDOW, EventId::new())
                .is_none()
        );
        clock.advance(WINDOW);

        assert!(
            map.remaining_or_stamp(user_key(instance, "live"), WINDOW, EventId::new())
                .is_none()
        );
        assert_eq!(
            map.entries.len(),
            1,
            "the dead entry must be dropped on the next cooldown-path touch, not held to the cap"
        );
    }

    #[test]
    fn releasing_with_the_stamping_event_reopens_the_window() {
        let mut map = CooldownMap::new(16);
        let key = user_key(TriggerInstanceId::new(), "alice");
        let stamping = EventId::new();
        map.remaining_or_stamp(key.clone(), WINDOW, stamping);

        map.release(&key, stamping);

        assert!(
            map.remaining_or_stamp(key, WINDOW, EventId::new())
                .is_none()
        );
    }

    #[test]
    fn releasing_with_a_foreign_event_keeps_the_window_spent() {
        let mut map = CooldownMap::new(16);
        let key = user_key(TriggerInstanceId::new(), "alice");
        map.remaining_or_stamp(key.clone(), WINDOW, EventId::new());

        map.release(&key, EventId::new());

        assert!(
            map.remaining_or_stamp(key, WINDOW, EventId::new())
                .is_some()
        );
    }

    #[test]
    fn releasing_one_user_leaves_another_users_window_spent() {
        let mut map = CooldownMap::new(16);
        let instance = TriggerInstanceId::new();
        let event = EventId::new();
        let (alice, bob) = (user_key(instance, "alice"), user_key(instance, "bob"));
        map.remaining_or_stamp(alice.clone(), WINDOW, event);
        map.remaining_or_stamp(bob.clone(), WINDOW, event);

        map.release(&alice, event);

        assert!(
            map.remaining_or_stamp(bob, WINDOW, EventId::new())
                .is_some()
        );
    }
}
