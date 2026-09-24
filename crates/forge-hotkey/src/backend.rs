use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HotkeyId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HotkeyEdge {
    Press,
    Release,
}

pub(crate) struct HotkeyFiredEvent {
    pub(crate) id: HotkeyId,
    pub(crate) combo: HotkeyCombo,
    pub(crate) timestamp_us: u64,
    pub(crate) edge: HotkeyEdge,
}

pub(crate) struct NullBackend {
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

impl NullBackend {
    pub(crate) fn new() -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyFiredEvent>(1);
        Self {
            fired_rx_slot: Mutex::new(Some(rx)),
        }
    }
}

#[async_trait]
impl HotkeyBackend for NullBackend {
    async fn register(&self, _id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        let _ = combo;
        Err(HotkeyError::PermissionDenied)
    }

    async fn unregister(&self, _id: HotkeyId) -> Result<(), HotkeyError> {
        Err(HotkeyError::PermissionDenied)
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

#[async_trait]
pub(crate) trait HotkeyBackend: Send + Sync {
    async fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError>;
    async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError>;
    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>>;

    /// True when the OS-level grab survives the client's disable/enable cycle untouched
    /// (the portal session stays bound to avoid a re-prompt); the client only gates delivery.
    fn delivery_gate_only(&self) -> bool {
        false
    }

    /// Signals a re-established backend session, after which any key-up the backend was
    /// holding is lost; the client closes its open holds on each notice.
    fn restart_rx(&self) -> Option<mpsc::Receiver<()>> {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    pub(crate) struct MockPortalBackend {
        pub(crate) registered: Arc<Mutex<HashMap<u32, String>>>,
        pub(crate) fail_on: Arc<Mutex<HashSet<String>>>,
        pub(crate) register_calls: Arc<AtomicUsize>,
        pub(crate) unregister_calls: Arc<AtomicUsize>,
        pub(crate) fail_unregister: Arc<AtomicBool>,
        pub(crate) restart_tx: mpsc::Sender<()>,
        gate_only: bool,
        fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
        restart_rx_slot: Mutex<Option<mpsc::Receiver<()>>>,
    }

    impl MockPortalBackend {
        pub(crate) fn new() -> (Self, mpsc::Sender<HotkeyFiredEvent>) {
            Self::with_gate(false)
        }

        pub(crate) fn new_delivery_gate_only() -> (Self, mpsc::Sender<HotkeyFiredEvent>) {
            Self::with_gate(true)
        }

        fn with_gate(gate_only: bool) -> (Self, mpsc::Sender<HotkeyFiredEvent>) {
            let (tx, rx) = mpsc::channel(64);
            let (restart_tx, restart_rx) = mpsc::channel(4);
            let mock = Self {
                registered: Arc::new(Mutex::new(HashMap::new())),
                fail_on: Arc::new(Mutex::new(HashSet::new())),
                register_calls: Arc::new(AtomicUsize::new(0)),
                unregister_calls: Arc::new(AtomicUsize::new(0)),
                fail_unregister: Arc::new(AtomicBool::new(false)),
                restart_tx,
                gate_only,
                fired_rx_slot: Mutex::new(Some(rx)),
                restart_rx_slot: Mutex::new(Some(restart_rx)),
            };
            (mock, tx)
        }

        pub(crate) fn add_conflict(&self, combo: &str) {
            self.fail_on.lock().unwrap().insert(combo.to_owned());
        }
    }

    #[async_trait]
    impl HotkeyBackend for MockPortalBackend {
        async fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
            self.register_calls.fetch_add(1, Ordering::Relaxed);
            let combo_str = combo.as_str().to_owned();
            if self.fail_on.lock().unwrap().contains(&combo_str) {
                return Err(HotkeyError::AlreadyRegistered { combo: combo_str });
            }
            self.registered.lock().unwrap().insert(id.0, combo_str);
            Ok(())
        }

        async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
            self.unregister_calls.fetch_add(1, Ordering::Relaxed);
            if self.fail_unregister.load(Ordering::Relaxed) {
                return Err(HotkeyError::MainThreadUnavailable);
            }
            self.registered.lock().unwrap().remove(&id.0);
            Ok(())
        }

        fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
            self.fired_rx_slot.lock().unwrap().take()
        }

        fn delivery_gate_only(&self) -> bool {
            self.gate_only
        }

        fn restart_rx(&self) -> Option<mpsc::Receiver<()>> {
            self.restart_rx_slot.lock().unwrap().take()
        }
    }

    pub(crate) struct MockFailAllBackend {
        fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
    }

    impl MockFailAllBackend {
        pub(crate) fn new() -> Self {
            let (_tx, rx) = mpsc::channel::<HotkeyFiredEvent>(1);
            Self {
                fired_rx_slot: Mutex::new(Some(rx)),
            }
        }
    }

    #[async_trait]
    impl HotkeyBackend for MockFailAllBackend {
        async fn register(&self, _id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
            let _ = combo;
            Err(HotkeyError::PermissionDenied)
        }

        async fn unregister(&self, _id: HotkeyId) -> Result<(), HotkeyError> {
            Err(HotkeyError::PermissionDenied)
        }

        fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
            self.fired_rx_slot.lock().unwrap().take()
        }
    }
}
