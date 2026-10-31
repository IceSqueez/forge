use std::time::Duration;

use time::OffsetDateTime;

use forge_platform_core::{
    BuiltinStatus, CapabilityFlags, CollectionRevisions, ConnectionState, HeaderAction,
};
use forge_types::{IntegrationId, MessageRef, RecoverRwLock, UserText, VerbatimText};

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
        self.last_failure.read_recover().clone()
    }

    fn uptime(&self) -> Option<Duration> {
        let at = (*self.connected_at.read_recover())?;
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

    fn status_revisions(&self) -> Option<CollectionRevisions> {
        Some(self.state.status_revisions())
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{BuiltinStatus, ConnectionState};

    use crate::client::VTubeClient;

    #[test]
    fn uptime_none_when_not_connected() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert!(s.uptime().is_none());
    }

    #[test]
    fn a_connection_change_moves_the_status_revision_and_a_repeat_does_not() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        let mut revisions = s
            .status_revisions()
            .expect("vtube declares a status signal");
        let moved = |revisions: &mut forge_platform_core::CollectionRevisions| {
            futures_util::FutureExt::now_or_never(revisions.changed()).is_some()
        };

        c.state.store(ConnectionState::Disconnected);
        assert!(!moved(&mut revisions), "same state");

        c.state.store(ConnectionState::Connected);
        assert!(moved(&mut revisions), "connected");
    }
}
