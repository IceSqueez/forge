use std::sync::atomic::Ordering;
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
        if !self.enabled.load(Ordering::Relaxed) {
            return ConnectionState::Disconnected;
        }
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

    use crate::backend::tests::MockPortalBackend;
    use crate::client::HotkeyClient;
    use crate::client::tests::{disable_and_settle, noop_publisher, start_supervised};
    use crate::combo::HotkeyCombo;

    #[tokio::test]
    async fn connection_connected_after_register() {
        let c = HotkeyClient::new_for_test(Some(true));
        let s: &dyn BuiltinStatus = &*c;
        c.register(HotkeyCombo::parse("Ctrl+G").unwrap())
            .await
            .unwrap();
        assert_eq!(s.connection(), ConnectionState::Connected);
    }

    #[tokio::test]
    async fn connection_reports_disconnected_while_the_engine_is_disabled() {
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, noop_publisher());
        client
            .register(HotkeyCombo::parse("Ctrl+G").unwrap())
            .await
            .unwrap();
        let status: &dyn BuiltinStatus = &*client;

        disable_and_settle(&client).await;
        assert_eq!(status.connection(), ConnectionState::Disconnected);

        client.enable().await.unwrap();
        assert_eq!(status.connection(), ConnectionState::Connected);
    }
}
