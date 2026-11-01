use forge_types::{IntegrationId, RecoverMutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use forge_events::EventPublisher;
use forge_net::{TaskOwner, TaskShutdown};

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
const SUPERVISOR_STOP_GRACE: Duration = Duration::from_millis(20);

const CONTROL_CAPACITY: usize = 8;
const MERGED_INPUT_CAPACITY: usize = 256;

pub struct MidiClient {
    id: IntegrationId,
    config: MidiConfig,
    backend: Arc<dyn MidiBackend>,
    publisher: Arc<dyn EventPublisher>,
    health_state: Arc<Mutex<MidiHealthSnapshot>>,
    health_tx: HealthTx,
    content_state: Arc<Mutex<MidiContentSnapshot>>,
    enabled: Arc<AtomicBool>,
    control_tx: mpsc::Sender<SupervisorCommand>,
    monitor_tx: MonitorTx,
    tasks: TaskOwner,
}

impl MidiClient {
    pub(crate) fn integration_id(&self) -> &IntegrationId {
        &self.id
    }

    pub(crate) fn config(&self) -> &MidiConfig {
        &self.config
    }

    pub(crate) fn backend(&self) -> &Arc<dyn MidiBackend> {
        &self.backend
    }

    pub(crate) fn publisher(&self) -> &Arc<dyn EventPublisher> {
        &self.publisher
    }

    pub(crate) fn health_state(&self) -> &Arc<Mutex<MidiHealthSnapshot>> {
        &self.health_state
    }

    pub(crate) fn health_tx(&self) -> &HealthTx {
        &self.health_tx
    }

    pub(crate) fn content_state(&self) -> &Arc<Mutex<MidiContentSnapshot>> {
        &self.content_state
    }

    pub(crate) fn enabled(&self) -> &Arc<AtomicBool> {
        &self.enabled
    }

    pub(crate) fn monitor_tx(&self) -> &MonitorTx {
        &self.monitor_tx
    }

    pub(crate) fn start(
        config: MidiConfig,
        publisher: Arc<dyn EventPublisher>,
        backend: Arc<dyn MidiBackend>,
    ) -> Arc<Self> {
        let (health_tx, health_state) = make_health_state();
        let content_state = make_content_state();
        let (control_tx, control_rx) = mpsc::channel::<SupervisorCommand>(CONTROL_CAPACITY);

        let client = Arc::new(Self {
            id: crate::MIDI_INTEGRATION.id,
            config,
            backend,
            publisher,
            health_state,
            health_tx,
            content_state,
            enabled: Arc::new(AtomicBool::new(true)),
            control_tx,
            monitor_tx: make_monitor_state(),
            tasks: TaskOwner::new(),
        });

        let (merged_tx, merged_rx) =
            mpsc::channel::<crate::supervisor::RawEvent>(MERGED_INPUT_CAPACITY);
        let client_ref = Arc::downgrade(&client);
        client.tasks.spawn(async move {
            run_supervisor(client_ref, merged_tx, merged_rx, control_rx).await;
        });

        client
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn start_with_backend(
        config: MidiConfig,
        publisher: Arc<dyn EventPublisher>,
        backend: Arc<crate::testing::MockMidiBackend>,
    ) -> Arc<Self> {
        Self::start(config, publisher, backend)
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
        let backend = Arc::clone(&self.backend);
        let port = port_name.to_owned();
        tokio::task::spawn_blocking(move || backend.send_output(&port, &bytes))
            .await
            .map_err(|e| MidiError::OutputSend(e.to_string()))?
    }

    pub async fn disable_input(&self) -> Result<(), MidiError> {
        self.send_command(SupervisorCommand::Disable).await
    }

    pub async fn enable_input(&self) -> Result<(), MidiError> {
        self.send_command(SupervisorCommand::Enable).await
    }

    pub async fn shutdown(&self) -> Result<(), MidiError> {
        let requested = self.send_command(SupervisorCommand::Shutdown).await;
        if let TaskShutdown::Aborted { tasks } = self.tasks.shutdown(SUPERVISOR_STOP_GRACE).await {
            tracing::warn!(
                tasks,
                "midi supervisor did not stop in time and was aborted"
            );
        }
        requested
    }

    pub async fn rescan_ports(&self) -> Result<(), MidiError> {
        self.send_command(SupervisorCommand::Rescan).await
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn connected_input_ports(&self) -> Vec<String> {
        let snap = self.content_state.lock_recover();
        snap.input_ports.iter().map(|p| p.name.clone()).collect()
    }

    pub fn connected_output_ports(&self) -> Vec<String> {
        let snap = self.content_state.lock_recover();
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
        let (control_tx, _control_rx) = mpsc::channel::<SupervisorCommand>(CONTROL_CAPACITY);
        Self::with_control_for_test(control_tx)
    }

    #[cfg(test)]
    pub(crate) fn with_control_for_test(control_tx: mpsc::Sender<SupervisorCommand>) -> Arc<Self> {
        use crate::testing::MockMidiBackend;
        use forge_events::test_support::NullPublisher;

        let (health_tx, health_state) = make_health_state();
        Arc::new(Self {
            id: crate::MIDI_INTEGRATION.id,
            config: MidiConfig::default(),
            backend: Arc::new(MockMidiBackend::new(vec![], vec![])),
            publisher: Arc::new(NullPublisher),
            health_state,
            health_tx,
            content_state: make_content_state(),
            enabled: Arc::new(AtomicBool::new(true)),
            control_tx,
            monitor_tx: make_monitor_state(),
            tasks: TaskOwner::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{input_port, output_port};

    fn client_holding_control_rx() -> (Arc<MidiClient>, mpsc::Receiver<SupervisorCommand>) {
        let (control_tx, control_rx) = mpsc::channel::<SupervisorCommand>(CONTROL_CAPACITY);
        (MidiClient::with_control_for_test(control_tx), control_rx)
    }

    #[tokio::test]
    async fn command_with_no_supervisor_listening_reports_supervisor_unavailable() {
        let client = MidiClient::new_for_test();
        let result = client.disable_input().await;
        assert!(matches!(result, Err(MidiError::SupervisorUnavailable)));
    }

    #[tokio::test(start_paused = true)]
    async fn command_to_a_wedged_supervisor_gives_up_instead_of_blocking_forever() {
        let (client, _control_rx) = client_holding_control_rx();
        for _ in 0..8 {
            client.rescan_ports().await.unwrap();
        }

        let result = client.rescan_ports().await;

        assert!(matches!(result, Err(MidiError::SupervisorUnavailable)));
    }

    #[test]
    fn connected_port_getters_do_not_cross_directions() {
        let client = MidiClient::new_for_test();
        {
            let mut snap = client.content_state.lock_recover();
            snap.input_ports = vec![input_port("Keys In")];
            snap.output_ports = vec![output_port("Synth Out")];
        }

        assert_eq!(client.connected_input_ports(), vec!["Keys In".to_owned()]);
        assert_eq!(
            client.connected_output_ports(),
            vec!["Synth Out".to_owned()]
        );
    }
}
