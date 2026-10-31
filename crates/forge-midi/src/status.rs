use forge_types::{IntegrationId, RecoverMutex, UserText};
use std::sync::atomic::Ordering;
use std::time::Duration;

use forge_platform_core::{
    BuiltinStatus, CapabilityFlags, ConnectionState, CredentialState, HeaderAction,
};

use crate::client::MidiClient;

impl BuiltinStatus for MidiClient {
    fn id(&self) -> &IntegrationId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "MIDI"
    }

    fn version(&self) -> Option<String> {
        None
    }

    fn connection(&self) -> ConnectionState {
        if !self.enabled.load(Ordering::Relaxed) {
            return ConnectionState::Disconnected;
        }
        let snap = self.content_state.lock_recover();
        if snap.input_ports.is_empty() && snap.output_ports.is_empty() {
            ConnectionState::Disconnected
        } else {
            ConnectionState::Connected
        }
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<UserText> {
        None
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn activity_count(&self) -> Option<usize> {
        let snap = self.content_state.lock_recover();
        Some(snap.input_ports.len())
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![]
    }

    fn credential_state(&self) -> CredentialState {
        CredentialState::Valid
    }

    fn status_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::tests::input_port;

    #[test]
    fn connection_is_disconnected_while_input_is_disabled_despite_open_ports() {
        let client = MidiClient::new_for_test();
        client.content_state.lock_recover().input_ports = vec![input_port("Piano")];

        let status: &dyn BuiltinStatus = &*client;
        assert_eq!(status.connection(), ConnectionState::Connected);

        client.enabled.store(false, Ordering::Relaxed);
        assert_eq!(status.connection(), ConnectionState::Disconnected);
    }
}
