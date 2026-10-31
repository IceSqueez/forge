#![cfg(feature = "testing")]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::EventPublisher;
use tokio::sync::mpsc;

use crate::backend::{HotkeyBackend, HotkeyEdge, HotkeyFiredEvent, HotkeyId};
use crate::client::HotkeyClient;
use crate::combo::HotkeyCombo;
use crate::config::HotkeyConfig;
use crate::error::HotkeyError;
use crate::main_thread::MainThreadLink;

const FIRED_CHANNEL_CAPACITY: usize = 64;
const UNREGISTERED_ID: HotkeyId = HotkeyId(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiredEdge {
    Press,
    Release,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordedCall {
    Register(HotkeyId, HotkeyCombo),
    Unregister(HotkeyId),
}

pub struct RecordingBackend {
    calls: Mutex<Vec<RecordedCall>>,
    fail_combos: Mutex<HashSet<HotkeyCombo>>,
    fired_tx: mpsc::Sender<HotkeyFiredEvent>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
}

impl RecordingBackend {
    fn new() -> Arc<Self> {
        let (fired_tx, rx) = mpsc::channel::<HotkeyFiredEvent>(FIRED_CHANNEL_CAPACITY);
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail_combos: Mutex::new(HashSet::new()),
            fired_tx,
            fired_rx_slot: Mutex::new(Some(rx)),
        })
    }

    pub fn fail_next_register(&self, combo: &HotkeyCombo) {
        self.fail_combos
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(combo.clone());
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub async fn fire(&self, combo: &HotkeyCombo, edge: FiredEdge) {
        let delivered = self
            .fired_tx
            .send(HotkeyFiredEvent {
                id: self.latest_id_of(combo),
                combo: combo.clone(),
                timestamp_us: 0,
                edge: match edge {
                    FiredEdge::Press => HotkeyEdge::Press,
                    FiredEdge::Release => HotkeyEdge::Release,
                },
            })
            .await;
        assert!(delivered.is_ok(), "the supervisor stopped reading edges");
    }

    fn latest_id_of(&self, combo: &HotkeyCombo) -> HotkeyId {
        self.calls()
            .iter()
            .rev()
            .find_map(|call| match call {
                RecordedCall::Register(id, registered) if registered == combo => Some(*id),
                _ => None,
            })
            .unwrap_or(UNREGISTERED_ID)
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

pub fn offline_main_thread_link() -> MainThreadLink {
    MainThreadLink::offline()
}

pub fn test_client(
    config: HotkeyConfig,
    publisher: Arc<dyn EventPublisher>,
) -> (Arc<HotkeyClient>, Arc<RecordingBackend>) {
    let backend = RecordingBackend::new();
    let backend_for_client: Arc<dyn HotkeyBackend> = backend.clone();
    let client = HotkeyClient::start(config, publisher, backend_for_client, Some(true));
    (client, backend)
}
