use crate::bus_adapter::BusAdapter;
use crate::server_info::ServerInfo;

pub struct EventFilterSnapshot {
    pub source: String,
    pub kind: String,
}

pub struct ConnectedClientSnapshot {
    pub identification: String,
    pub remote_addr: String,
    pub client_type: String,
    pub subscriptions: Vec<EventFilterSnapshot>,
    pub events_per_second: f32,
    pub uptime_seconds: i64,
}

pub struct BandwidthSnapshot {
    pub outbound_bytes_per_second: u64,
    pub outbound_bytes_total: u64,
    pub peak_outbound_bytes_per_second: u64,
}

pub struct ServerSnapshot {
    pub uptime_seconds: i64,
    pub connected_clients: Vec<ConnectedClientSnapshot>,
    pub bandwidth: BandwidthSnapshot,
    pub aggregate_events_per_second: f32,
    pub http_requests_total: u64,
    pub events_out_total: u64,
    pub dropped_events_total: u64,
}

pub(crate) async fn build_server_snapshot(
    server_info: &ServerInfo,
    bus_adapter: &BusAdapter,
) -> ServerSnapshot {
    let connected_clients = connected_clients_snapshot(server_info, bus_adapter).await;
    let aggregate_events_per_second = connected_clients.iter().map(|c| c.events_per_second).sum();
    let bw = &server_info.bandwidth;

    ServerSnapshot {
        uptime_seconds: server_info.uptime_seconds(),
        connected_clients,
        bandwidth: BandwidthSnapshot {
            outbound_bytes_per_second: bw.current_bps(),
            outbound_bytes_total: bw.total(),
            peak_outbound_bytes_per_second: bw.peak(),
        },
        aggregate_events_per_second,
        http_requests_total: server_info.http_requests(),
        events_out_total: server_info.events_out(),
        dropped_events_total: server_info.dropped_events(),
    }
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
                )
            })
            .collect()
    };

    let mut result = Vec::with_capacity(rows.len());
    for (id, identification, client_type, remote_addr, events_per_second, uptime_seconds) in rows {
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
            identification: identification.as_str().to_owned(),
            remote_addr,
            client_type: client_type.type_str().to_owned(),
            subscriptions,
            events_per_second,
            uptime_seconds,
        });
    }
    result
}
