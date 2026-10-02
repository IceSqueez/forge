use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::client::MidiClient;
use crate::error::MidiError;
use crate::events::MidiOutMessage;

#[async_trait]
pub trait MidiSink: Send + Sync {
    async fn send_output(&self, port_name: &str, message: &MidiOutMessage)
    -> Result<(), MidiError>;
}

#[async_trait]
impl MidiSink for MidiClient {
    async fn send_output(
        &self,
        port_name: &str,
        message: &MidiOutMessage,
    ) -> Result<(), MidiError> {
        self.send_output(port_name, message).await
    }
}

#[derive(Default)]
pub struct SwitchableMidiSink {
    live: RwLock<Option<Arc<MidiClient>>>,
}

impl SwitchableMidiSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn install(&self, client: Arc<MidiClient>) {
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(client);
    }

    pub fn take(&self) -> Option<Arc<MidiClient>> {
        let mut guard = self.live.write().unwrap_or_else(|e| e.into_inner());
        guard.take()
    }

    pub fn live(&self) -> Option<Arc<MidiClient>> {
        let guard = self.live.read().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }
}

#[async_trait]
impl MidiSink for SwitchableMidiSink {
    async fn send_output(
        &self,
        port_name: &str,
        message: &MidiOutMessage,
    ) -> Result<(), MidiError> {
        let client = self.live().ok_or(MidiError::SupervisorUnavailable)?;
        client.send_output(port_name, message).await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct NoopSink;

    #[async_trait]
    impl MidiSink for NoopSink {
        async fn send_output(
            &self,
            _port_name: &str,
            _message: &MidiOutMessage,
        ) -> Result<(), MidiError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn switchable_sink_reaches_only_the_currently_installed_engine() {
        use crate::backend::MidiBackend;
        use crate::backend::tests::MockMidiBackend;
        use crate::config::MidiConfig;
        use crate::events::{MidiPortInfo, PortDirection};

        struct NoopPublisher;
        impl forge_events::EventPublisher for NoopPublisher {
            fn publish(&self, _: forge_events::Event) {}
        }

        let note = MidiOutMessage::NoteOn {
            note: 60,
            velocity: 100,
            channel: 0,
        };
        let backend = Arc::new(MockMidiBackend::new(
            vec![],
            vec![MidiPortInfo {
                name: "Out".to_owned(),
                direction: PortDirection::Output,
            }],
        ));
        let sink = SwitchableMidiSink::new();

        let before = sink.send_output("Out", &note).await;
        sink.install(MidiClient::start(
            MidiConfig::default(),
            Arc::new(NoopPublisher),
            Arc::clone(&backend) as Arc<dyn MidiBackend>,
        ));
        let installed = sink.send_output("Out", &note).await;
        let taken = sink.take();
        let after = sink.send_output("Out", &note).await;

        assert!(matches!(before, Err(MidiError::SupervisorUnavailable)));
        assert!(installed.is_ok());
        assert!(taken.is_some());
        assert!(matches!(after, Err(MidiError::SupervisorUnavailable)));
        assert_eq!(backend.sent_outputs().len(), 1);
    }
}
