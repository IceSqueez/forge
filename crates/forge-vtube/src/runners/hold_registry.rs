use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use tokio::task::JoinHandle;

pub(crate) struct HoldRegistry {
    holds: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl HoldRegistry {
    pub(crate) fn new() -> Self {
        Self {
            holds: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, JoinHandle<()>>> {
        self.holds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn abort(&self, param_id: &str) {
        let mut holds = self.lock();
        holds.retain(|_, handle| !handle.is_finished());
        if let Some(previous) = holds.remove(param_id) {
            previous.abort();
        }
    }

    pub(crate) fn insert(&self, param_id: String, handle: JoinHandle<()>) {
        let mut holds = self.lock();
        holds.retain(|_, handle| !handle.is_finished());
        holds.insert(param_id, handle);
    }

    pub(crate) fn abort_all(&self) {
        let mut holds = self.lock();
        for (_, handle) in holds.drain() {
            handle.abort();
        }
    }
}
