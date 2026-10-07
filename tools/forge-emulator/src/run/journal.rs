use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use forge_events::Event;
use forge_types::EventId;

use crate::control::{EventStream, Observation};

#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub arrived: Instant,
    pub observation: Observation,
}

#[derive(Clone, Default)]
pub struct Journal {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    inner: Mutex<Inner>,
    changes: watch::Sender<u64>,
}

#[derive(Default)]
struct Inner {
    entries: Vec<JournalEntry>,
    seeded: HashSet<EventId>,
    closed_at: Option<Instant>,
}

pub struct JournalView<'a> {
    pub entries: &'a [JournalEntry],
    pub closed_at: Option<Instant>,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn follow(stream: EventStream) -> (Self, JoinHandle<()>) {
        let journal = Self::new();
        let task = journal.attach(stream);
        (journal, task)
    }

    pub fn attach(&self, stream: EventStream) -> JoinHandle<()> {
        let feeder = self.clone();
        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(observation) = stream.next().await {
                feeder.record(observation);
            }
            feeder.close();
        })
    }

    pub fn seed(&self, events: Vec<Event>) {
        self.mutate(|inner| {
            for event in events {
                if inner.seeded.insert(event.id) {
                    inner.entries.push(JournalEntry {
                        arrived: Instant::now(),
                        observation: Observation::Event(event),
                    });
                }
            }
        });
    }

    pub fn record(&self, observation: Observation) {
        self.mutate(|inner| {
            if let Observation::Event(event) = &observation
                && !event.replay
                && inner.seeded.remove(&event.id)
            {
                return;
            }
            inner.entries.push(JournalEntry {
                arrived: Instant::now(),
                observation,
            });
        });
    }

    pub fn close(&self) {
        self.mutate(|inner| {
            inner.closed_at.get_or_insert_with(Instant::now);
        });
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read<R>(&self, f: impl FnOnce(JournalView<'_>) -> R) -> R {
        let inner = self.lock();
        f(JournalView {
            entries: &inner.entries,
            closed_at: inner.closed_at,
        })
    }

    pub async fn wait_until(
        &self,
        deadline: Instant,
        mut settled: impl FnMut(JournalView<'_>) -> bool,
    ) {
        let mut changes = self.shared.changes.subscribe();
        loop {
            let done = self.read(|view| view.closed_at.is_some() || settled(view));
            if done || Instant::now() >= deadline {
                return;
            }
            if !matches!(
                tokio::time::timeout_at(deadline, changes.changed()).await,
                Ok(Ok(()))
            ) {
                return;
            }
        }
    }

    fn mutate(&self, f: impl FnOnce(&mut Inner)) {
        f(&mut self.lock());
        self.shared
            .changes
            .send_modify(|generation| *generation += 1);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.shared
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_events::EventSource;
    use serde_json::json;
    use time::macros::datetime;

    use super::*;

    fn event(replay: bool) -> Event {
        Event {
            id: EventId::new(),
            source: EventSource::Core,
            kind: "donation.received".to_owned(),
            timestamp: datetime!(2026-10-03 12:00:00 UTC),
            payload: json!({}),
            caused_by: None,
            replay,
            causation_depth: 0,
        }
    }

    #[test]
    fn pushed_copy_of_a_seeded_event_is_recorded_once() {
        let journal = Journal::new();
        let seeded = event(false);
        journal.seed(vec![seeded.clone()]);
        journal.record(Observation::Event(seeded.clone()));
        journal.record(Observation::Event(event(false)));

        assert_eq!(journal.len(), 2);
    }

    #[test]
    fn replay_of_a_seeded_event_is_still_recorded() {
        let journal = Journal::new();
        let seeded = event(false);
        journal.seed(vec![seeded.clone()]);
        let mut replayed = seeded;
        replayed.replay = true;
        journal.record(Observation::Event(replayed));

        assert_eq!(journal.len(), 2);
    }

    #[test]
    fn seeding_the_same_event_twice_keeps_one_entry() {
        let journal = Journal::new();
        let seeded = event(false);
        journal.seed(vec![seeded.clone(), seeded]);

        assert_eq!(journal.len(), 1);
    }
}
