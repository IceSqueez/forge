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
    // Snapshot the per-client synchronous data under the read guard, then
    // release it before issuing any async bus-adapter lookups so client
    // register/deregister writers are not blocked for the whole loop.
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
