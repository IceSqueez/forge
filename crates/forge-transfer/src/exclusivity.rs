use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default)]
pub struct TransferExclusivity {
    held: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct TransferGuard {
    held: Arc<AtomicBool>,
}

impl TransferExclusivity {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_acquire(&self) -> Option<TransferGuard> {
        self.held
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| TransferGuard {
                held: Arc::clone(&self.held),
            })
    }

    pub fn is_held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.held.store(false, Ordering::Release);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::TransferExclusivity;

    #[test]
    fn a_second_acquire_fails_while_the_first_guard_lives() {
        let exclusivity = TransferExclusivity::new();
        let _guard = exclusivity.try_acquire();

        assert!(exclusivity.try_acquire().is_none());
    }

    #[test]
    fn a_guard_taken_through_one_clone_blocks_every_other_clone_until_dropped() {
        let scheduler = TransferExclusivity::new();
        let manual = scheduler.clone();
        let guard = scheduler.try_acquire().unwrap();

        let while_held = (manual.is_held(), manual.try_acquire().is_some());
        drop(guard);
        let after_release = (manual.is_held(), manual.try_acquire().is_some());

        assert_eq!((while_held, after_release), ((true, false), (false, true)));
    }
}
