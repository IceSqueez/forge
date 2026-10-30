use std::time::Duration;

use time::OffsetDateTime;

use forge_platform_core::{BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction};
use forge_types::{IntegrationId, MessageRef, UserText, VerbatimText};

use crate::client::VTubeClient;

impl BuiltinStatus for VTubeClient {
    fn id(&self) -> &IntegrationId {
        &self.vtube_id
    }

    fn display_name(&self) -> &str {
        "VTube Studio"
    }

    fn version(&self) -> Option<String> {
        self.vtube_version.get().cloned()
    }

    fn connection(&self) -> ConnectionState {
        self.connection_state()
    }

    fn connect_failure(&self) -> Option<MessageRef> {
        self.last_failure.read().ok().and_then(|g| g.clone())
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

    fn endpoint(&self) -> Option<UserText> {
        Some(VerbatimText::new(self.config.endpoint.as_str()).into())
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::Disconnect]
    }
}

#[cfg(test)]
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
