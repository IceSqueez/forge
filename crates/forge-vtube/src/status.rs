use std::time::Duration;

use time::OffsetDateTime;

use forge_platform_core::{
    BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction,
};

use crate::client::VTubeClient;

impl BuiltinStatus for VTubeClient {
    fn id(&self) -> &BuiltinId {
        &self.vtube_id
    }

    fn display_name(&self) -> &str {
        "VTube Studio"
    }

    fn version(&self) -> Option<&str> {
        self.vtube_version.get().map(|s| s.as_str())
    }

    fn connection(&self) -> ConnectionState {
        self.connection_state()
    }

    fn uptime(&self) -> Option<Duration> {
        let at = self.connected_at.read().ok().and_then(|g| *g)?;
        let elapsed = OffsetDateTime::now_utc() - at;
        if elapsed.is_positive() {
            Some(elapsed.unsigned_abs())
        } else {
            None
        }
    }

    fn endpoint(&self) -> Option<&str> {
        Some(&self.config.endpoint)
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::Reconnect, HeaderAction::Disconnect]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{BuiltinStatus, HeaderAction};

    use crate::client::VTubeClient;

    #[test]
    fn id_is_vtube() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert_eq!(s.id().as_str(), "vtube");
    }

    #[test]
    fn display_name_is_vtube_studio() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert_eq!(s.display_name(), "VTube Studio");
    }

    #[test]
    fn version_none_before_connect() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert!(s.version().is_none());
    }

    #[test]
    fn endpoint_reflects_config() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:9001/");
        let s: &dyn BuiltinStatus = &c;
        assert_eq!(s.endpoint(), Some("ws://127.0.0.1:9001/"));
    }

    #[test]
    fn capability_flags_not_limited() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        let flags = s.capability_flags();
        assert!(!flags.limited);
        assert!(flags.label.is_none());
    }

    #[test]
    fn header_actions_contains_reconnect_and_disconnect() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        let actions = s.header_actions();
        assert!(actions.contains(&HeaderAction::Reconnect));
        assert!(actions.contains(&HeaderAction::Disconnect));
        assert!(!actions.contains(&HeaderAction::RefreshToken));
    }

    #[test]
    fn uptime_none_when_not_connected() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert!(s.uptime().is_none());
    }
}
