#![cfg(feature = "testing")]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::EventPublisher;
use tokio::sync::mpsc;

use crate::backend::{HotkeyBackend, HotkeyFiredEvent, HotkeyId};
use crate::client::HotkeyClient;
use crate::combo::HotkeyCombo;
use crate::config::HotkeyConfig;
use crate::error::HotkeyError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordedCall {
    Register(HotkeyId, HotkeyCombo),
    Unregister(HotkeyId),
}

pub struct RecordingBackend {
    calls: Mutex<Vec<RecordedCall>>,
    fail_combos: Mutex<HashSet<HotkeyCombo>>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

impl RecordingBackend {
    fn new() -> Arc<Self> {
        let (_tx, rx) = mpsc::channel::<HotkeyFiredEvent>(1);
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail_combos: Mutex::new(HashSet::new()),
            fired_rx_slot: Mutex::new(Some(rx)),
        })
    }

    /// Makes the next `register` of this combo fail as though the OS already held it, then
    /// clears itself so a later retry of the same combo succeeds.
    pub fn fail_next_register(&self, combo: &HotkeyCombo) {
        self.fail_combos
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(combo.clone());
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

#[async_trait]
impl HotkeyBackend for RecordingBackend {
    async fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        let should_fail = self
            .fail_combos
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(combo);
        if should_fail {
            return Err(HotkeyError::AlreadyRegistered {
                combo: combo.as_str().to_owned(),
            });
        }
        self.calls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(RecordedCall::Register(id, combo.clone()));
        Ok(())
    }

    async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        self.calls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(RecordedCall::Unregister(id));
        Ok(())
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

/// Builds a `HotkeyClient` on a fresh `RecordingBackend` with no D-Bus or `/dev/input` access,
/// for dependent crates that need to drive registration ordering without OS state.
pub fn test_client(
    config: HotkeyConfig,
    publisher: Arc<dyn EventPublisher>,
) -> (Arc<HotkeyClient>, Arc<RecordingBackend>) {
    let backend = RecordingBackend::new();
    let backend_for_client: Arc<dyn HotkeyBackend> = backend.clone();
    let client = HotkeyClient::start(config, publisher, backend_for_client, Some(true));
    (client, backend)
}
