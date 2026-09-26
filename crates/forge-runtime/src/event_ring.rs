use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use forge_events::Event;
use forge_types::EventId;

pub(crate) struct EventRing {
    events: VecDeque<Arc<Event>>,
    first_seq: u64,
    by_id: HashMap<EventId, u64>,
    retention: usize,
}

impl EventRing {
    pub(crate) fn new(retention: usize) -> Self {
        Self {
            events: VecDeque::new(),
            first_seq: 0,
            by_id: HashMap::new(),
            retention: retention.max(1),
        }
    }

    pub(crate) fn push(&mut self, event: Arc<Event>) {
        if self.events.len() == self.retention
            && let Some(evicted) = self.events.pop_front()
        {
            if self.by_id.get(&evicted.id) == Some(&self.first_seq) {
                self.by_id.remove(&evicted.id);
            }
            self.first_seq += 1;
        }
        let seq = self.first_seq + self.events.len() as u64;
        self.by_id.insert(event.id, seq);
        self.events.push_back(event);
    }

    /// The newest retained event carrying `id`.
    pub(crate) fn get(&self, id: EventId) -> Option<&Arc<Event>> {
        self.position(id).and_then(|index| self.events.get(index))
    }

    pub(crate) fn position(&self, id: EventId) -> Option<usize> {
        let seq = *self.by_id.get(&id)?;
        usize::try_from(seq.checked_sub(self.first_seq)?).ok()
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Arc<Event>> {
        self.events.iter()
    }

    pub(crate) fn after(&self, index: usize) -> impl DoubleEndedIterator<Item = &Arc<Event>> {
        self.events
            .range(index.saturating_add(1).min(self.events.len())..)
    }
}

