use forge_types::{IntegrationId, RecoverMutex, UserText};
use std::time::Duration;

use forge_platform_core::{
    BuiltinStatus, CapabilityFlags, ConnectionState, CredentialState, HeaderAction,
};

use crate::client::DiscordClient;

impl BuiltinStatus for DiscordClient {
    fn id(&self) -> &IntegrationId {
        self.integration_id()
    }

    fn display_name(&self) -> &str {
        "Discord"
    }

    fn version(&self) -> Option<String> {
        None
    }

    fn connection(&self) -> ConnectionState {
        if self.has_webhooks() {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnected
        }
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<UserText> {
        None
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn activity_count(&self) -> Option<usize> {
        let snap = self.content_state().lock_recover();
        (!snap.registry.is_empty()).then_some(snap.registry.len())
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::Settings]
    }

    fn credential_state(&self) -> CredentialState {
        CredentialState::Valid
    }

    fn status_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
        Some(self.status_signal().subscribe())
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{BuiltinStatus, ConnectionState};

    use crate::client::DiscordClient;
    use std::sync::Arc;

    use forge_storage::test_support::MemoryCredentials;

    #[test]
    fn connection_disconnected_when_no_webhooks() {
        let c = DiscordClient::new_for_test();
        let s: &dyn BuiltinStatus = &*c;
        assert_eq!(s.connection(), ConnectionState::Disconnected);
    }

    async fn moved(revisions: &mut forge_platform_core::CollectionRevisions) -> bool {
        tokio::select! {
            biased;
            _ = revisions.changed() => true,
            () = std::future::ready(()) => false,
        }
    }

    #[tokio::test]
    async fn every_webhook_list_change_signals_the_status_it_decides() {
        const URL: &str = "https://discord.com/api/webhooks/1/token";
        let creds = Arc::new(MemoryCredentials::default());
        creds.put("discord:stored", "{}");
        let client = DiscordClient::new_for_test_with_creds(creds);
        let mut revisions = client.status_revisions().unwrap();

        client.list_webhooks().await.unwrap();
        assert!(moved(&mut revisions).await, "startup listing");
        assert_eq!(client.connection(), ConnectionState::Connected);

        client.save_webhook("alerts", URL).await.unwrap();
        assert!(moved(&mut revisions).await, "webhook added");
        assert_eq!(client.connection(), ConnectionState::Connected);

        client.delete_webhook("alerts").await.unwrap();
        assert!(moved(&mut revisions).await, "one of two deleted");
        assert_eq!(client.connection(), ConnectionState::Connected);

        client.delete_webhook("stored").await.unwrap();
        assert!(moved(&mut revisions).await, "last webhook deleted");
        assert_eq!(client.connection(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn the_activity_count_shows_configured_webhooks_and_hides_when_there_are_none() {
        for (webhooks, expected) in [(&[][..], None), (&["alerts", "clips"][..], Some(2))] {
            let creds = Arc::new(MemoryCredentials::default());
            for name in webhooks {
                creds.put(&format!("discord:{name}"), "{}");
            }
            let client = DiscordClient::new_for_test_with_creds(creds.clone());
            client.list_webhooks().await.unwrap();

            let status: &dyn BuiltinStatus = &*client;
            assert_eq!(status.activity_count(), expected, "{webhooks:?}");
        }
    }
}
