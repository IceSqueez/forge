use std::time::Duration;

use forge_platform_core::{
    BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction,
};

use crate::client::HotkeyClient;

impl BuiltinStatus for HotkeyClient {
    fn id(&self) -> &BuiltinId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Hotkeys"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        let snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
        if snap.registered_count > 0 {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnected
        }
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<&str> {
        Some(self.config.app_name.as_str())
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

    use crate::client::HotkeyClient;

    #[test]
    fn id_is_hotkey() {
        let c = HotkeyClient::new_for_test(None);
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.id().as_str(), "hotkey");
    }

    #[test]
    fn display_name_is_hotkeys() {
        let c = HotkeyClient::new_for_test(None);
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.display_name(), "Hotkeys");
    }

    #[test]
    fn connection_disconnected_with_no_registered() {
        let c = HotkeyClient::new_for_test(None);
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.connection(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn connection_connected_after_register() {
        use crate::combo::HotkeyCombo;
        let c = HotkeyClient::new_for_test(Some(true));
        let s: &dyn BuiltinStatus = &*c;
        let combo = HotkeyCombo::parse("Ctrl+G").unwrap();
        c.register(combo).await.unwrap();
        assert_eq!(s.connection(), ConnectionState::Connected);
    }

    #[test]
    fn capability_flags_not_limited() {
        let c = HotkeyClient::new_for_test(None);
        let s: &dyn BuiltinStatus = &*c;
        assert!(!s.capability_flags().limited);
    }

    #[test]
    fn version_is_none() {
        let c = HotkeyClient::new_for_test(None);
        let s: &dyn BuiltinStatus = &*c;
        assert!(s.version().is_none());
    }
}
