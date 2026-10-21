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
