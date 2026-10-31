use std::time::Duration;

use forge_platform_core::{
    BuiltinStatus, CapabilityFlags, CollectionRevisions, ConnectionState, CredentialState,
    HeaderAction,
};
use forge_types::{IntegrationId, MessageRef, RecoverRwLock, UserText, VerbatimText};

use crate::client::VTubeClient;
use crate::vocabulary::messages;

fn credential_state_after(failure: Option<&MessageRef>) -> CredentialState {
    let Some(failure) = failure else {
        return CredentialState::Valid;
    };
    if *failure == messages::token_rejected() {
        CredentialState::Rejected
    } else if *failure == messages::access_denied() || *failure == messages::approval_timeout() {
        CredentialState::ReauthRequired
    } else {
        CredentialState::Valid
    }
}

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
        self.connected_at.read_recover().map(|at| at.elapsed())
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

    fn credential_state(&self) -> CredentialState {
        credential_state_after(self.last_failure.read_recover().as_ref())
    }

    fn status_revisions(&self) -> Option<CollectionRevisions> {
        Some(self.state.status_revisions())
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{BuiltinStatus, ConnectionState, CredentialState};

    use super::credential_state_after;
    use crate::client::VTubeClient;
    use crate::vocabulary::messages;

    #[test]
    fn a_failure_maps_to_the_credential_state_it_implies() {
        for (failure, expected) in [
            (None, CredentialState::Valid),
            (Some(messages::unreachable()), CredentialState::Valid),
            (Some(messages::token_rejected()), CredentialState::Rejected),
            (
                Some(messages::access_denied()),
                CredentialState::ReauthRequired,
            ),
            (
                Some(messages::approval_timeout()),
                CredentialState::ReauthRequired,
            ),
        ] {
            assert_eq!(
                credential_state_after(failure.as_ref()),
                expected,
                "{failure:?}"
            );
        }
    }

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
