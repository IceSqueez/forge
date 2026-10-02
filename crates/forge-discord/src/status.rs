use std::time::Duration;

use forge_platform_core::{BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction};
use forge_types::IntegrationId;

use crate::client::DiscordClient;

impl BuiltinStatus for DiscordClient {
    fn id(&self) -> &IntegrationId {
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

    fn activity_count(&self) -> Option<usize> {
        let snap = self.content_state.lock().unwrap_or_else(|p| p.into_inner());
        (!snap.webhook_names.is_empty()).then_some(snap.webhook_names.len())
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
    use crate::client::tests::MockCreds;

    #[test]
    fn connection_disconnected_when_no_webhooks() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.connection(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn the_activity_count_shows_configured_webhooks_and_hides_when_there_are_none() {
        for (webhooks, expected) in [(&[][..], None), (&["alerts", "clips"][..], Some(2))] {
            let creds = MockCreds::new();
            for name in webhooks {
                creds.insert(&format!("discord:{name}"), "{}");
            }
            let client = DiscordClient::new_for_test_with_creds(creds.creds());
            client.list_webhooks().await.unwrap();

            let status: &dyn BuiltinStatus = &*client;
            assert_eq!(status.activity_count(), expected, "{webhooks:?}");
        }
    }
}
