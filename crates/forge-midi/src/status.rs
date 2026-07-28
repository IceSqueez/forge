use std::sync::atomic::Ordering;
use std::time::Duration;

use forge_platform_core::{
    BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction,
};

use crate::client::MidiClient;

impl BuiltinStatus for MidiClient {
    fn id(&self) -> &BuiltinId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "MIDI"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        if !self.enabled.load(Ordering::Relaxed) {
            return ConnectionState::Disconnected;
        }
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        if snap.input_ports.is_empty() && snap.output_ports.is_empty() {
            ConnectionState::Disconnected
        } else {
            ConnectionState::Connected
        }
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<&str> {
        None
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::events::{MidiPortInfo, PortDirection};

    #[test]
    fn connection_is_disconnected_while_input_is_disabled_despite_open_ports() {
        let client = MidiClient::new_for_test();
        client
            .content_state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .input_ports = vec![MidiPortInfo {
            name: "Piano".to_owned(),
            direction: PortDirection::Input,
        }];

        let status: &dyn BuiltinStatus = &*client;
        assert_eq!(status.connection(), ConnectionState::Connected);

        client.enabled.store(false, Ordering::Relaxed);
        assert_eq!(status.connection(), ConnectionState::Disconnected);
    }
}
