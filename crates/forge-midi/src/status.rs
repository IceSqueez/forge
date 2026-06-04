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
    use forge_platform_core::{BuiltinStatus, ConnectionState};

    use crate::client::MidiClient;

    #[test]
    fn id_is_midi() {
        let c = MidiClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.id().as_str(), "midi");
    }

    #[test]
    fn display_name_is_midi() {
        let c = MidiClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.display_name(), "MIDI");
    }

    #[test]
    fn connection_disconnected_with_no_ports() {
        let c = MidiClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.connection(), ConnectionState::Disconnected);
    }

    #[test]
    fn capability_flags_not_limited() {
        let c = MidiClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert!(!s.capability_flags().limited);
    }

    #[test]
    fn version_is_none() {
        let c = MidiClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert!(s.version().is_none());
    }
}
