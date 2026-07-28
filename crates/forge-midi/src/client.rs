use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use forge_events::EventPublisher;
use forge_platform_core::BuiltinId;

use crate::backend::{MidiBackend, MidirBackend};
use crate::config::MidiConfig;
use crate::content::{MidiContentSnapshot, make_content_state};
use crate::decode::message_to_bytes;
use crate::error::MidiError;
use crate::events::MidiOutMessage;
use crate::health::{HealthTx, MidiHealthSnapshot, make_health_state};
use crate::monitor::{MidiMonitorStream, MonitorTx, make_monitor_state};
use crate::supervisor::{SupervisorCommand, run_supervisor};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

pub struct MidiClient {
    pub(crate) id: BuiltinId,
    pub(crate) config: MidiConfig,
    pub(crate) backend: Arc<dyn MidiBackend>,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) health_state: Arc<Mutex<MidiHealthSnapshot>>,
    pub(crate) health_tx: HealthTx,
    pub(crate) content_state: Arc<Mutex<MidiContentSnapshot>>,
    pub(crate) enabled: Arc<AtomicBool>,
    pub(crate) control_tx: mpsc::Sender<SupervisorCommand>,
    pub(crate) monitor_tx: MonitorTx,
}

impl MidiClient {
    pub(crate) fn start(
        config: MidiConfig,
        publisher: Arc<dyn EventPublisher>,
        backend: Arc<dyn MidiBackend>,
    ) -> Arc<Self> {
        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();
        let (control_tx, control_rx) = mpsc::channel::<SupervisorCommand>(8);

        let client = Arc::new(Self {
            id: BuiltinId::new("midi"),
            config,
            backend,
            publisher,
            health_state,
            health_tx,
            content_state,
            enabled: Arc::new(AtomicBool::new(true)),
            control_tx,
            monitor_tx: make_monitor_state(),
        });

        let (merged_tx, merged_rx) = mpsc::channel::<crate::supervisor::RawEvent>(256);
        let client_ref = Arc::clone(&client);
        tokio::spawn(async move {
            run_supervisor(client_ref, merged_tx, merged_rx, control_rx).await;
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

    pub async fn disable_input(&self) -> Result<(), MidiError> {
        self.send_command(SupervisorCommand::Disable).await
    }

    pub async fn enable_input(&self) -> Result<(), MidiError> {
        self.send_command(SupervisorCommand::Enable).await
    }

    pub async fn rescan_ports(&self) -> Result<(), MidiError> {
        self.send_command(SupervisorCommand::Rescan).await
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn connected_input_ports(&self) -> Vec<String> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        snap.input_ports.iter().map(|p| p.name.clone()).collect()
    }

    pub fn connected_output_ports(&self) -> Vec<String> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        snap.output_ports.iter().map(|p| p.name.clone()).collect()
    }

    pub fn monitor_stream(&self) -> MidiMonitorStream {
        crate::monitor::subscribe(&self.monitor_tx)
    }

    async fn send_command(&self, cmd: SupervisorCommand) -> Result<(), MidiError> {
        tokio::time::timeout(COMMAND_TIMEOUT, self.control_tx.send(cmd))
            .await
            .map_err(|_| MidiError::SupervisorUnavailable)?
            .map_err(|_| MidiError::SupervisorUnavailable)
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
        let (control_tx, _control_rx) = mpsc::channel::<SupervisorCommand>(8);
        Arc::new(Self {
            id: BuiltinId::new("midi"),
            config: MidiConfig::default(),
            backend,
            publisher: Arc::new(NoopPublisher),
            health_state,
            health_tx,
            content_state: make_content_state(),
            enabled: Arc::new(AtomicBool::new(true)),
            control_tx,
            monitor_tx: make_monitor_state(),
        })
    }
}
