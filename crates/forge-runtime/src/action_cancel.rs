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

    /// Keyed per-execution, so deregistering one finished run never strands a concurrent run.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_trips_only_signals_under_the_named_action_and_returns_their_count() {
        let registry = ActionCancelRegistry::new();
        let a = ActionId::new();
        let b = ActionId::new();
        let (a1, a2, other) = (
            CancelSignal::new(),
            CancelSignal::new(),
            CancelSignal::new(),
        );
        registry.register(a, a1.clone());
        registry.register(a, a2.clone());
        registry.register(b, other.clone());

        assert_eq!(registry.cancel(a), 2);
        assert!(a1.is_cancelled());
        assert!(a2.is_cancelled());
        assert!(
            !other.is_cancelled(),
            "cancelling action a must not touch a signal filed under action b"
        );
    }

    #[test]
    fn deregister_of_one_execution_leaves_a_concurrent_run_of_the_same_action_cancellable() {
        let registry = ActionCancelRegistry::new();
        let action = ActionId::new();
        let finished = CancelSignal::new();
        let still_running = CancelSignal::new();
        let finished_exec = registry.register(action, finished.clone());
        let running_exec = registry.register(action, still_running.clone());
        assert_ne!(
            finished_exec, running_exec,
            "each execution must get a distinct id"
        );

        registry.deregister(action, finished_exec);

        assert_eq!(registry.cancel(action), 1);
        assert!(still_running.is_cancelled());
        assert!(
            !finished.is_cancelled(),
            "the deregistered execution must no longer be cancellable"
        );
    }

    #[test]
    fn cancel_and_cancel_all_on_empty_registry_return_zero() {
        let registry = ActionCancelRegistry::new();
        assert_eq!(registry.cancel(ActionId::new()), 0);
        assert_eq!(registry.cancel_all(), 0);
    }

    #[test]
    fn cancel_all_trips_every_signal_across_all_actions() {
        let registry = ActionCancelRegistry::new();
        let a = ActionId::new();
        let b = ActionId::new();
        let signals = [
            CancelSignal::new(),
            CancelSignal::new(),
            CancelSignal::new(),
        ];
        registry.register(a, signals[0].clone());
        registry.register(a, signals[1].clone());
        registry.register(b, signals[2].clone());

        assert_eq!(registry.cancel_all(), 3);
        assert!(signals.iter().all(CancelSignal::is_cancelled));
    }
}
