use std::collections::HashMap;
use std::time::{Duration, Instant};

use forge_types::{EventId, TriggerInstanceId};

pub type CooldownKey = (TriggerInstanceId, Option<String>);

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

pub struct CooldownMap {
    entries: HashMap<CooldownKey, CooldownEntry>,
    capacity: usize,
}

impl CooldownMap {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn remaining_or_stamp(
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

    fn evict_oldest(&mut self) {
        // Dropping a still-live entry lets that chatter through one invocation early - the bound
        // trades unbounded growth for the same benign under-throttle a restart already produces.
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
