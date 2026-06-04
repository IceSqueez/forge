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
}
