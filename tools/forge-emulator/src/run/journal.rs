use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::control::{EventStream, Observation};

#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub arrived: Instant,
    pub observation: Observation,
}

/// Every observation the control connection delivered, in arrival order; nothing is ever removed.
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

    /// Records everything `stream` yields until it ends, then marks the journal closed.
    pub fn follow(stream: EventStream) -> (Self, JoinHandle<()>) {
        let journal = Self::new();
        let feeder = journal.clone();
        let task = tokio::spawn(async move {
            let mut stream = stream;
            while let Some(observation) = stream.next().await {
                feeder.record(observation);
            }
            feeder.close();
        });
        (journal, task)
    }

    /// Stamped under the lock, so a read at any instant sees every entry stamped by then.
    pub fn record(&self, observation: Observation) {
        self.mutate(|inner| {
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

    /// The index the next recorded observation will take.
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

    /// Returns once `settled` holds, the journal closes, or `deadline` passes, whichever is first.
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
