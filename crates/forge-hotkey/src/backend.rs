use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HotkeyId(pub u32);

#[allow(dead_code)]
pub(crate) struct HotkeyFiredEvent {
    pub(crate) id: HotkeyId,
    pub(crate) combo: HotkeyCombo,
    pub(crate) timestamp_us: u64,
}

#[allow(dead_code)]
pub(crate) struct NullBackend {
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

impl NullBackend {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        let (_tx, rx) = mpsc::channel::<HotkeyFiredEvent>(1);
        Self {
            fired_rx_slot: Mutex::new(Some(rx)),
        }
    }
}

impl HotkeyBackend for NullBackend {
    fn register(&self, _id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        let _ = combo;
        Err(HotkeyError::PermissionDenied)
    }

    fn unregister(&self, _id: HotkeyId) -> Result<(), HotkeyError> {
        Err(HotkeyError::PermissionDenied)
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

#[allow(dead_code)]
pub(crate) trait HotkeyBackend: Send + Sync {
    fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError>;
    fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError>;
    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use super::*;

    pub(crate) struct MockPortalBackend {
        pub(crate) registered: Arc<Mutex<HashMap<u32, String>>>,
        pub(crate) fail_on: Arc<Mutex<HashSet<String>>>,
        #[allow(dead_code)]
        pub(crate) fired_tx: mpsc::Sender<HotkeyFiredEvent>,
        fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
    }

    impl MockPortalBackend {
        pub(crate) fn new() -> (Self, mpsc::Sender<HotkeyFiredEvent>) {
            let (tx, rx) = mpsc::channel(64);
            let mock = Self {
                registered: Arc::new(Mutex::new(HashMap::new())),
                fail_on: Arc::new(Mutex::new(HashSet::new())),
                fired_tx: tx.clone(),
                fired_rx_slot: Mutex::new(Some(rx)),
            };
            (mock, tx)
        }

        pub(crate) fn add_conflict(&self, combo: &str) {
            self.fail_on.lock().unwrap().insert(combo.to_owned());
        }
    }

    impl HotkeyBackend for MockPortalBackend {
        fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
            let combo_str = combo.as_str().to_owned();
            if self.fail_on.lock().unwrap().contains(&combo_str) {
                return Err(HotkeyError::AlreadyRegistered { combo: combo_str });
            }
            self.registered.lock().unwrap().insert(id.0, combo_str);
            Ok(())
        }

        fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
            self.registered.lock().unwrap().remove(&id.0);
            Ok(())
        }

        fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
            self.fired_rx_slot.lock().unwrap().take()
        }
    }

    pub(crate) struct MockGlobalHotkeyBackend {
        pub(crate) registered: Arc<Mutex<HashMap<u32, String>>>,
        fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
        #[allow(dead_code)]
        pub(crate) fired_tx: mpsc::Sender<HotkeyFiredEvent>,
    }

    impl MockGlobalHotkeyBackend {
        pub(crate) fn new() -> (Self, mpsc::Sender<HotkeyFiredEvent>) {
            let (tx, rx) = mpsc::channel(64);
            let mock = Self {
                registered: Arc::new(Mutex::new(HashMap::new())),
                fired_rx_slot: Mutex::new(Some(rx)),
                fired_tx: tx.clone(),
            };
            (mock, tx)
        }
    }

    impl HotkeyBackend for MockGlobalHotkeyBackend {
        fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
            self.registered
                .lock()
                .unwrap()
                .insert(id.0, combo.as_str().to_owned());
            Ok(())
        }

        fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
            self.registered.lock().unwrap().remove(&id.0);
            Ok(())
        }

        fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
            self.fired_rx_slot.lock().unwrap().take()
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

    impl HotkeyBackend for MockFailAllBackend {
        fn register(&self, _id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
            let _ = combo;
            Err(HotkeyError::PermissionDenied)
        }

        fn unregister(&self, _id: HotkeyId) -> Result<(), HotkeyError> {
            Err(HotkeyError::PermissionDenied)
        }

        fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
            self.fired_rx_slot.lock().unwrap().take()
        }
    }

    #[test]
    fn hotkey_id_serde_roundtrip() {
        let id = HotkeyId(42);
        let json = serde_json::to_string(&id).unwrap();
        let back: HotkeyId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn mock_portal_register_stores_combo() {
        let (mock, _tx) = MockPortalBackend::new();
        let combo = HotkeyCombo::parse("Ctrl+A").unwrap();
        mock.register(HotkeyId(1), &combo).unwrap();
        let reg = mock.registered.lock().unwrap();
        assert_eq!(reg.get(&1), Some(&"Ctrl+A".to_owned()));
    }

    #[test]
    fn mock_portal_conflict_returns_already_registered() {
        let (mock, _tx) = MockPortalBackend::new();
        mock.add_conflict("Ctrl+A");
        let combo = HotkeyCombo::parse("Ctrl+A").unwrap();
        let err = mock.register(HotkeyId(1), &combo).unwrap_err();
        assert!(matches!(err, HotkeyError::AlreadyRegistered { .. }));
    }

    #[test]
    fn mock_portal_unregister_removes_entry() {
        let (mock, _tx) = MockPortalBackend::new();
        let combo = HotkeyCombo::parse("Ctrl+A").unwrap();
        mock.register(HotkeyId(1), &combo).unwrap();
        mock.unregister(HotkeyId(1)).unwrap();
        assert!(mock.registered.lock().unwrap().is_empty());
    }

    #[test]
    fn mock_portal_fired_rx_returns_some_once() {
        let (mock, _tx) = MockPortalBackend::new();
        assert!(mock.fired_rx().is_some());
        assert!(mock.fired_rx().is_none());
    }

    #[test]
    fn mock_fail_all_register_returns_permission_denied() {
        let mock = MockFailAllBackend::new();
        let combo = HotkeyCombo::parse("Ctrl+A").unwrap();
        let err = mock.register(HotkeyId(1), &combo).unwrap_err();
        assert!(matches!(err, HotkeyError::PermissionDenied));
    }

    #[test]
    fn mock_global_hotkey_registers_and_unregisters() {
        let (mock, _tx) = MockGlobalHotkeyBackend::new();
        let combo = HotkeyCombo::parse("Alt+F4").unwrap();
        mock.register(HotkeyId(5), &combo).unwrap();
        assert!(mock.registered.lock().unwrap().contains_key(&5));
        mock.unregister(HotkeyId(5)).unwrap();
        assert!(!mock.registered.lock().unwrap().contains_key(&5));
    }
}
