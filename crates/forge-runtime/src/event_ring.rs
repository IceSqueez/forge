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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::EventSource;

    use super::*;

    const MAX_RETENTION: usize = 5;
    const MAX_PUSHES: usize = 13;

    fn event() -> Arc<Event> {
        Arc::new(Event::new(
            EventSource::Core,
            "ring.probe",
            serde_json::Value::Null,
        ))
    }

    fn ids<'a>(events: impl Iterator<Item = &'a Arc<Event>>) -> Vec<EventId> {
        events.map(|event| event.id).collect()
    }

    /// The linear scans the index replaced: newest match by id, then everything after it.
    fn scanned(window: &[Arc<Event>], id: EventId) -> Option<(usize, Vec<EventId>)> {
        let position = window.iter().rposition(|event| event.id == id)?;
        Some((position, ids(window[position + 1..].iter())))
    }

    fn indexed(ring: &EventRing, id: EventId) -> Option<(usize, Vec<EventId>)> {
        let position = ring.position(id)?;
        Some((position, ids(ring.after(position))))
    }

    #[test]
    fn indexed_lookup_matches_a_linear_scan_of_the_retained_window() {
        for retention in 1..=MAX_RETENTION {
            for pushes in 0..=MAX_PUSHES {
                let pushed: Vec<Arc<Event>> = (0..pushes).map(|_| event()).collect();
                let mut ring = EventRing::new(retention);
                for event in &pushed {
                    ring.push(Arc::clone(event));
                }
                let window = &pushed[pushes.saturating_sub(retention)..];

                for probe in pushed.iter().map(|event| event.id).chain([EventId::new()]) {
                    assert_eq!(
                        (indexed(&ring, probe), ring.get(probe).map(|event| event.id)),
                        (
                            scanned(window, probe),
                            scanned(window, probe).map(|_| probe)
                        ),
                        "retention {retention}, {pushes} pushes"
                    );
                }
                assert_eq!(ids(ring.iter()), ids(window.iter()));
            }
        }
    }

    #[test]
    fn evicting_an_older_copy_of_an_id_keeps_the_newer_copy_reachable() {
        let repeated = event();
        let mut ring = EventRing::new(3);
        for event in [
            Arc::clone(&repeated),
            event(),
            Arc::clone(&repeated),
            event(),
        ] {
            ring.push(event);
        }

        assert_eq!(ring.position(repeated.id), Some(1));
    }
}
