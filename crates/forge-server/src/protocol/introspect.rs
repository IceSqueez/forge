use std::sync::atomic::Ordering;

use forge_storage::CredentialsRepo;

use crate::bus_adapter::BusAdapter;
use crate::server_info::ServerInfo;

const PLATFORM_PREFIXES: &[&str] = &["twitch:", "youtube:", "kick:"];

pub(crate) async fn build_connected_accounts(
    creds: &dyn CredentialsRepo,
) -> Vec<serde_json::Value> {
    let ids = match creds.list_ids().await {
        Ok(ids) => ids,
        Err(_) => return vec![],
    };
    ids.into_iter()
        .filter_map(|id| {
            let s = id.as_str();
            for prefix in PLATFORM_PREFIXES {
                if let Some(login) = s.strip_prefix(prefix) {
                    let platform = &prefix[..prefix.len() - 1];
                    return Some(serde_json::json!({
                        "platform": platform,
                        "login": login,
                    }));
                }
            }
            None
        })
        .collect()
}

pub(crate) async fn build_connected_clients(
    server_info: &ServerInfo,
    bus_adapter: &BusAdapter,
) -> Vec<serde_json::Value> {
    // Snapshot under the read guard, then release it before any async bus-adapter lookups.
    let snapshots: Vec<_> = {
        let clients = server_info.connected_clients.read().await;
        clients
            .iter()
            .map(|(id, client)| {
                (
                    *id,
                    client.identification.load_full(),
                    client.client_type.load_full(),
                    client.remote_addr.ip().to_string(),
                    client.events_per_second(),
                    client.uptime().whole_seconds(),
                    client.bytes_sent_session.load(Ordering::Relaxed),
                )
            })
            .collect()
    };

    let mut result = Vec::with_capacity(snapshots.len());
    for (
        id,
        identification,
        client_type,
        remote_addr,
        events_per_second,
        uptime_seconds,
        bytes_sent,
    ) in snapshots
    {
        let subs = bus_adapter.current_subscriptions(id).await;
        let subscriptions: Vec<serde_json::Value> = subs
            .iter()
            .map(|f| {
                let source_str = match f.source {
                    None => "*".to_owned(),
                    Some(s) => serde_json::to_value(s)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "*".to_owned()),
                };
                let kind_str = f.kind.as_deref().unwrap_or("*");
                serde_json::json!({ "source": source_str, "type": kind_str })
            })
            .collect();

        result.push(serde_json::json!({
            "client_id": id.to_string(),
            "identification": identification.as_str(),
            "remote_addr": remote_addr,
            "client_type": client_type.type_str(),
            "subscriptions": subscriptions,
            "events_per_second": events_per_second,
            "uptime_seconds": uptime_seconds,
            "bytes_sent": bytes_sent,
        }));
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use forge_events::EventSource;
    use forge_runtime::{EventBus, NullEventLogRepo};

    use super::build_connected_clients;
    use crate::bus_adapter::{BusAdapter, ClientFilterSet, EventFilter};
    use crate::server_info::ServerInfo;
    use crate::ws_client::WsClient;

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

        let info = ServerInfo::new();
        info.register(handle.id, client).await;

        let rows = build_connected_clients(&info, &adapter).await;
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

    #[tokio::test]
    async fn client_absent_from_bus_adapter_yields_empty_subscriptions() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let adapter = BusAdapter::new(bus);
        let (handle, _rx) = adapter
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        adapter.unregister_client(handle.id).await;

        let addr: std::net::SocketAddr = "203.0.113.9:6000".parse().unwrap();
        let client = Arc::new(WsClient::new(handle.id, addr, Arc::new(AtomicU64::new(0))));
        let info = ServerInfo::new();
        info.register(handle.id, client).await;

        let rows = build_connected_clients(&info, &adapter).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["subscriptions"].as_array().unwrap().len(), 0);
    }
}
