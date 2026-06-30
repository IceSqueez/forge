use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use forge_registry::CancelSignal;
use forge_types::ActionId;

#[derive(Clone, Default)]
pub struct ActionCancelRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

#[derive(Default)]
struct RegistryInner {
    next_exec_id: u64,
    by_action: HashMap<ActionId, HashMap<u64, CancelSignal>>,
}

impl ActionCancelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The returned id identifies this single execution within its action's
    /// inner map, so deregistering one finished run never strands a concurrent
    /// run of the same action.
    pub fn register(&self, action_id: ActionId, signal: CancelSignal) -> u64 {
        let mut inner = self.lock();
        let exec_id = inner.next_exec_id;
        inner.next_exec_id = inner.next_exec_id.wrapping_add(1);
        inner
            .by_action
            .entry(action_id)
            .or_default()
            .insert(exec_id, signal);
        exec_id
    }

    pub fn deregister(&self, action_id: ActionId, exec_id: u64) {
        let mut inner = self.lock();
        if let Some(execs) = inner.by_action.get_mut(&action_id) {
            execs.remove(&exec_id);
            if execs.is_empty() {
                inner.by_action.remove(&action_id);
            }
        }
    }

    pub fn cancel(&self, action_id: ActionId) -> usize {
        let inner = self.lock();
        match inner.by_action.get(&action_id) {
            Some(execs) => {
                for signal in execs.values() {
                    signal.cancel();
                }
                execs.len()
            }
            None => 0,
        }
    }

    pub fn cancel_all(&self) -> usize {
        let inner = self.lock();
        let mut count = 0;
        for execs in inner.by_action.values() {
            for signal in execs.values() {
                signal.cancel();
                count += 1;
            }
        }
        count
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RegistryInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}
