use std::collections::HashMap;
use std::time::{Duration, Instant};

use forge_types::{EventId, TriggerInstanceId};

pub(crate) type CooldownKey = (TriggerInstanceId, Option<String>);

struct CooldownEntry {
    stamped_at: Instant,
    window: Duration,
    event_id: EventId,
}

impl CooldownEntry {
    fn is_expired(&self) -> bool {
        self.stamped_at.elapsed() >= self.window
    }
}

pub(crate) struct CooldownMap {
    entries: HashMap<CooldownKey, CooldownEntry>,
    capacity: usize,
}

impl CooldownMap {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
        }
    }

    pub(crate) fn remaining_or_stamp(
        &mut self,
        key: CooldownKey,
        window: Duration,
        event_id: EventId,
    ) -> Option<Duration> {
        if let Some(entry) = self.entries.get(&key) {
            if entry.event_id == event_id {
                return None;
            }
            let elapsed = entry.stamped_at.elapsed();
            if elapsed < window {
                return Some(window - elapsed);
            }
        }

        self.entries.retain(|_, entry| !entry.is_expired());

        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.evict_oldest();
        }

        self.entries.insert(
            key,
            CooldownEntry {
                stamped_at: Instant::now(),
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

    fn user_key(instance: TriggerInstanceId, user: &str) -> CooldownKey {
        (instance, Some(user.to_owned()))
    }

    #[test]
    fn a_later_event_inside_the_window_reports_the_remaining_time() {
        let mut map = CooldownMap::new(16);
        let key = user_key(TriggerInstanceId::new(), "alice");

        assert!(
            map.remaining_or_stamp(key.clone(), WINDOW, EventId::new())
                .is_none(),
            "an unstamped key must pass and stamp"
        );

        let remaining = map
            .remaining_or_stamp(key, WINDOW, EventId::new())
            .expect("a second event inside the window must be throttled");
        assert!(
            remaining <= WINDOW && remaining > WINDOW / 2,
            "remaining {remaining:?} must be just under the full window"
        );
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
        let mut map = CooldownMap::new(2);
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
            std::thread::sleep(Duration::from_millis(1));
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
        let mut map = CooldownMap::new(1024);
        let instance = TriggerInstanceId::new();

        assert!(
            map.remaining_or_stamp(
                user_key(instance, "gone"),
                Duration::from_millis(5),
                EventId::new()
            )
            .is_none()
        );
        std::thread::sleep(Duration::from_millis(10));

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
