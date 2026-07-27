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
        // Reconnect is intentionally absent here; it stays reachable via auto-reconnect.
        vec![HeaderAction::Disconnect]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::BuiltinStatus;

    use crate::client::VTubeClient;

    #[test]
    fn uptime_none_when_not_connected() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert!(s.uptime().is_none());
    }
}
