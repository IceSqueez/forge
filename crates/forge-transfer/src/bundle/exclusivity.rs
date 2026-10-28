use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Notify, watch};

#[derive(Debug, Clone)]
pub struct TransferExclusivity {
    held: Arc<AtomicBool>,
    released: Arc<Notify>,
    held_signal: Arc<watch::Sender<bool>>,
}

#[derive(Debug)]
pub struct TransferGuard {
    held: Arc<AtomicBool>,
    released: Arc<Notify>,
    held_signal: Arc<watch::Sender<bool>>,
}

#[derive(Debug)]
pub struct TransferHeldWatch(watch::Receiver<bool>);

impl TransferHeldWatch {
    pub async fn changed(&mut self) -> Option<()> {
        self.0.changed().await.ok()
    }
}

impl Default for TransferExclusivity {
    fn default() -> Self {
        Self {
            held: Arc::default(),
            released: Arc::default(),
            held_signal: Arc::new(watch::Sender::new(false)),
        }
    }
}

impl TransferExclusivity {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_acquire(&self) -> Option<TransferGuard> {
        self.held
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| {
                self.held_signal.send_replace(true);
                TransferGuard {
                    held: Arc::clone(&self.held),
                    released: Arc::clone(&self.released),
                    held_signal: Arc::clone(&self.held_signal),
                }
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

    pub fn watch_held(&self) -> TransferHeldWatch {
        TransferHeldWatch(self.held_signal.subscribe())
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.held.store(false, Ordering::Release);
        self.held_signal.send_replace(false);
        self.released.notify_waiters();
    }
}
