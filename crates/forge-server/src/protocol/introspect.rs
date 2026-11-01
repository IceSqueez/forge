use forge_events::EventSource;
use forge_storage::CredentialsRepo;
use forge_types::PlatformId;

use crate::delivery::bus_adapter::BusAdapter;

const CREDENTIAL_ID_SEPARATOR: char = ':';
const WILDCARD: &str = "*";

pub(crate) async fn build_connected_accounts(
    creds: &dyn CredentialsRepo,
) -> Vec<serde_json::Value> {
    let ids = match creds.list_ids().await {
        Ok(ids) => ids,
        Err(error) => {
            tracing::warn!(%error, "connected accounts unreadable");
            return vec![];
        }
    };
    ids.into_iter()
        .filter_map(|id| {
            let (platform, login) = id.as_str().split_once(CREDENTIAL_ID_SEPARATOR)?;
            let platform = PlatformId::from_canonical(platform)?;
            Some(serde_json::json!({
                "platform": platform.as_str(),
                "login": login,
            }))
        })
        .collect()
}

pub(crate) async fn build_connected_clients(bus_adapter: &BusAdapter) -> Vec<serde_json::Value> {
    bus_adapter
        .client_rows()
        .await
        .into_iter()
        .map(|row| {
            let subscriptions: Vec<serde_json::Value> = row
                .subscriptions
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "source": f.source.map_or(WILDCARD, EventSource::as_str),
                        "type": f.kind.as_deref().unwrap_or(WILDCARD),
                    })
                })
                .collect();
            serde_json::json!({
                "client_id": row.id.to_string(),
                "identification": row.identification,
                "remote_addr": row.remote_addr,
                "client_type": row.client_type,
                "subscriptions": subscriptions,
                "events_per_second": row.events_per_second,
                "uptime_seconds": row.uptime_seconds,
                "bytes_sent": row.bytes_sent,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use forge_runtime::{EventBus, NullEventLogRepo};

    use super::{EventSource, build_connected_accounts, build_connected_clients};
    use crate::delivery::bus_adapter::{
        BusAdapter, ClientFilterSet, EventFilter, OverlayClientClass,
    };
    use crate::delivery::ws_client::WsClient;
    use crate::test_helpers::TestDataProvider;

    async fn accounts_for(ids: &[&'static str]) -> Vec<serde_json::Value> {
        let ids: Vec<forge_storage::CredentialId> = ids
            .iter()
            .map(|id| forge_storage::CredentialId::new(*id))
            .collect();
        let mut provider = TestDataProvider::new();
        provider
            .credentials()
            .expect_list_ids()
            .returning(move || Ok(ids.clone()));
        build_connected_accounts(&provider).await
    }

    #[tokio::test]
    async fn connected_accounts_split_credential_ids_at_the_first_colon_into_platform_and_login() {
        let accounts = accounts_for(&[
            "twitch:bot",
            "youtube:channel",
            "kick:a:b",
            "discord:webhook",
            "unknown:x",
            "twitchbot",
            "Twitch:caps",
            ":empty",
            "",
        ])
        .await;

        assert_eq!(
            serde_json::Value::Array(accounts),
            serde_json::json!([
                { "platform": "twitch", "login": "bot" },
                { "platform": "youtube", "login": "channel" },
                { "platform": "kick", "login": "a:b" },
            ])
        );
    }

    #[tokio::test]
    async fn connected_accounts_are_empty_when_the_credential_store_fails() {
        let mut provider = TestDataProvider::new();
        provider.credentials().expect_list_ids().returning(|| {
            Err(forge_storage::StorageError::Connection {
                reason: "down".to_owned(),
            })
        });

        assert!(build_connected_accounts(&provider).await.is_empty());
    }

    #[tokio::test]
    async fn the_wire_client_list_answers_with_the_browser_source_and_drops_the_preview_tab() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let adapter = BusAdapter::new(bus);
        let identity = forge_storage::OverlayId::new("goal-box");
        let addr: std::net::SocketAddr = "203.0.113.7:5555".parse().unwrap();

        let mut promoted = Vec::new();
        for (label, class) in [
            ("browser-source", OverlayClientClass::BrowserSource),
            ("preview-tab", OverlayClientClass::PreviewTab),
        ] {
            let (handle, _rx) = adapter
                .register_client(ClientFilterSet::new(HashSet::new()))
                .await;
            promoted.push(
                adapter
                    .promote_to_overlay(handle.id, identity.clone(), class)
                    .await
                    .expect("a just-registered client is still on the registry"),
            );
            let client = Arc::new(WsClient::new(handle.id, addr, Arc::new(AtomicU64::new(0))));
            client.identification.store(Arc::new(label.to_owned()));
            adapter.attach_session(handle.id, client).await;
        }

        let rows = build_connected_clients(&adapter).await;

        let listed: Vec<&str> = rows
            .iter()
            .filter_map(|row| row["identification"].as_str())
            .collect();
        assert_eq!(
            listed,
            vec!["browser-source"],
            "the answer a third-party dashboard reads must count browser sources only"
        );
    }

    #[tokio::test]
    async fn snapshot_row_preserves_identity_addr_and_subscriptions() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let adapter = BusAdapter::new(bus);
        let filters = ClientFilterSet::new(HashSet::from([EventFilter::new(
            Some(EventSource::Twitch),
            Some("chat.message".to_owned()),
        )]));
        let (handle, _rx) = adapter.register_client(filters).await;

        let addr: std::net::SocketAddr = "203.0.113.7:5555".parse().unwrap();
        let client = Arc::new(WsClient::new(handle.id, addr, Arc::new(AtomicU64::new(0))));
        client
            .identification
            .store(Arc::new("dashboard-1".to_owned()));

        adapter.attach_session(handle.id, client).await;

        let rows = build_connected_clients(&adapter).await;
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["client_id"], handle.id.to_string());
        assert_eq!(row["identification"], "dashboard-1");
        assert_eq!(row["remote_addr"], "203.0.113.7");

        let subs = row["subscriptions"].as_array().unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0]["source"], "twitch");
        assert_eq!(subs[0]["type"], "chat.message");
    }
}
