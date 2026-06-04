use std::time::Duration;

use forge_platform_core::{
    BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction,
};

use crate::client::DiscordClient;

impl BuiltinStatus for DiscordClient {
    fn id(&self) -> &BuiltinId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Discord"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        if snap.webhook_names.is_empty() {
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
        vec![HeaderAction::Settings]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{BuiltinStatus, ConnectionState};

    use crate::client::DiscordClient;

    #[test]
    fn id_is_discord() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.id().as_str(), "discord");
    }

    #[test]
    fn display_name_is_discord() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.display_name(), "Discord");
    }

    #[test]
    fn connection_disconnected_when_no_webhooks() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.connection(), ConnectionState::Disconnected);
    }

    #[test]
    fn capability_flags_not_limited() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        let flags = s.capability_flags();
        assert!(!flags.limited);
    }

    #[test]
    fn version_is_none() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert!(s.version().is_none());
    }
}
