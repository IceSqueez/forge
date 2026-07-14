use std::sync::atomic::Ordering;

use forge_storage::CredentialsRepo;

use crate::bus_adapter::BusAdapter;
use crate::server_info::ServerInfo;

const PLATFORM_PREFIXES: &[&str] = &["twitch:", "youtube:", "kick:"];

pub struct ConnectedAccountSnapshot {
    pub platform: String,
    pub login: String,
}

pub struct EventFilterSnapshot {
    pub source: String,
    pub kind: String,
}

pub struct ConnectedClientSnapshot {
    pub client_id: String,
    pub identification: String,
    pub remote_addr: String,
    pub client_type: String,
    pub subscriptions: Vec<EventFilterSnapshot>,
    pub events_per_second: f32,
    pub uptime_seconds: i64,
    pub bytes_sent: u64,
}

pub struct BandwidthSnapshot {
    pub outbound_bytes_per_second: u64,
    pub outbound_bytes_total: u64,
    pub peak_outbound_bytes_per_second: u64,
}

pub struct ServerSnapshot {
    pub version: &'static str,
    pub uptime_seconds: i64,
    pub connected_accounts: Vec<ConnectedAccountSnapshot>,
    pub available_platforms: Vec<&'static str>,
    pub connected_clients: Vec<ConnectedClientSnapshot>,
    pub bandwidth: BandwidthSnapshot,
    pub aggregate_events_per_second: f32,
}

pub(crate) async fn build_server_snapshot(
    server_info: &ServerInfo,
    bus_adapter: &BusAdapter,
    credentials: &dyn CredentialsRepo,
) -> ServerSnapshot {
    let connected_accounts = connected_accounts_snapshot(credentials).await;
    let connected_clients = connected_clients_snapshot(server_info, bus_adapter).await;
    let aggregate_events_per_second = connected_clients.iter().map(|c| c.events_per_second).sum();
    let bw = &server_info.bandwidth;

    ServerSnapshot {
        version: server_info.version,
        uptime_seconds: server_info.uptime_seconds(),
        connected_accounts,
        available_platforms: vec!["twitch"],
        connected_clients,
        bandwidth: BandwidthSnapshot {
            outbound_bytes_per_second: bw.current_bps(),
            outbound_bytes_total: bw.total(),
            peak_outbound_bytes_per_second: bw.peak(),
        },
        aggregate_events_per_second,
    }
}

async fn connected_accounts_snapshot(
    credentials: &dyn CredentialsRepo,
) -> Vec<ConnectedAccountSnapshot> {
    let ids = match credentials.list_ids().await {
        Ok(ids) => ids,
        Err(_) => return vec![],
    };
    ids.into_iter()
        .filter_map(|id| {
            let s = id.as_str();
            PLATFORM_PREFIXES.iter().find_map(|prefix| {
                s.strip_prefix(prefix)
                    .map(|login| ConnectedAccountSnapshot {
                        platform: prefix[..prefix.len() - 1].to_owned(),
                        login: login.to_owned(),
                    })
            })
        })
        .collect()
}

async fn connected_clients_snapshot(
    server_info: &ServerInfo,
    bus_adapter: &BusAdapter,
) -> Vec<ConnectedClientSnapshot> {
    let rows: Vec<_> = {
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

    let mut result = Vec::with_capacity(rows.len());
    for (
        id,
        identification,
        client_type,
        remote_addr,
        events_per_second,
        uptime_seconds,
        bytes_sent,
    ) in rows
    {
        let subs = bus_adapter.current_subscriptions(id).await;
        let subscriptions = subs
            .iter()
            .map(|f| EventFilterSnapshot {
                source: f
                    .source
                    .and_then(|s| serde_json::to_value(s).ok())
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "*".to_owned()),
                kind: f.kind.clone().unwrap_or_else(|| "*".to_owned()),
            })
            .collect();

        result.push(ConnectedClientSnapshot {
            client_id: id.to_string(),
            identification: identification.as_str().to_owned(),
            remote_addr,
            client_type: client_type.type_str().to_owned(),
            subscriptions,
            events_per_second,
            uptime_seconds,
            bytes_sent,
        });
    }
    result
}
