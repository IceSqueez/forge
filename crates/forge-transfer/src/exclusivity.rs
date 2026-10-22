use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

#[derive(Debug, Clone, Default)]
pub struct TransferExclusivity {
    held: Arc<AtomicBool>,
    released: Arc<Notify>,
}

#[derive(Debug)]
pub struct TransferGuard {
    held: Arc<AtomicBool>,
    released: Arc<Notify>,
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
                released: Arc::clone(&self.released),
            })
    }

    pub async fn acquire_when_free(&self) -> TransferGuard {
        loop {
            let released = self.released.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if let Some(guard) = self.try_acquire() {
                return guard;
            }
            released.await;
        }
    }

    pub fn is_held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.held.store(false, Ordering::Release);
        self.released.notify_waiters();
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
