use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::mpsc;

use forge_events::EventPublisher;
use forge_platform_core::BuiltinId;

use crate::backend::{MidiBackend, MidirBackend};
use crate::config::MidiConfig;
use crate::content::{MidiContentSnapshot, make_content_state};
use crate::decode::message_to_bytes;
use crate::error::MidiError;
use crate::events::{MidiOutMessage, MidiPortInfo};
use crate::health::{HealthTx, MidiHealthSnapshot, make_health_state};
use crate::supervisor::run_supervisor;

pub struct MidiClient {
    pub(crate) id: BuiltinId,
    pub(crate) config: MidiConfig,
    pub(crate) backend: Arc<dyn MidiBackend>,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) input_ports: RwLock<Vec<MidiPortInfo>>,
    pub(crate) output_ports: RwLock<Vec<MidiPortInfo>>,
    pub(crate) health_state: Arc<Mutex<MidiHealthSnapshot>>,
    pub(crate) health_tx: HealthTx,
    pub(crate) content_state: Arc<Mutex<MidiContentSnapshot>>,
}

impl MidiClient {
    pub(crate) fn start(
        config: MidiConfig,
        publisher: Arc<dyn EventPublisher>,
        backend: Arc<dyn MidiBackend>,
    ) -> Arc<Self> {
        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();

        let client = Arc::new(Self {
            id: BuiltinId::new("midi"),
            config,
            backend,
            publisher,
            input_ports: RwLock::new(Vec::new()),
            output_ports: RwLock::new(Vec::new()),
            health_state,
            health_tx,
            content_state,
        });

        let (merged_tx, merged_rx) = mpsc::channel::<crate::supervisor::RawEvent>(256);
        let client_ref = Arc::clone(&client);
        tokio::spawn(async move {
            run_supervisor(client_ref, merged_tx, merged_rx).await;
        });

        client
    }

    pub fn start_with_midir(
        config: MidiConfig,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Arc<Self>, MidiError> {
        let backend = Arc::new(MidirBackend::new(config.client_name.clone())?);
        Ok(Self::start(config, publisher, backend))
    }

    pub async fn send_output(
        &self,
        port_name: &str,
        message: &MidiOutMessage,
    ) -> Result<(), MidiError> {
        let bytes = message_to_bytes(message)?;
        self.backend.send_output(port_name, &bytes)
    }

    pub fn input_ports(&self) -> Vec<MidiPortInfo> {
        self.input_ports
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn output_ports(&self) -> Vec<MidiPortInfo> {
        self.output_ports
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn new_for_test() -> Arc<Self> {
        use crate::backend::tests::MockMidiBackend;

        struct NoopPublisher;
        impl EventPublisher for NoopPublisher {
            fn publish(&self, _: forge_events::Event) {}
        }

        let backend = Arc::new(MockMidiBackend::new(vec![], vec![]));
        let (health_tx, health_state) = make_health_state();
        Arc::new(Self {
            id: BuiltinId::new("midi"),
            config: MidiConfig::default(),
            backend,
            publisher: Arc::new(NoopPublisher),
            input_ports: RwLock::new(Vec::new()),
            output_ports: RwLock::new(Vec::new()),
            health_state,
            health_tx,
            content_state: make_content_state(),
        })
    }
}
