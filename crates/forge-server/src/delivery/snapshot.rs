use std::collections::HashSet;
use std::sync::atomic::Ordering;

use forge_events::EventSource;

use crate::delivery::bus_adapter::{BusAdapter, ClientId, EventFilter};
use crate::delivery::ws_client::WsClient;
use crate::lifecycle::server_info::ServerInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFilterSnapshot {
    pub source: Option<EventSource>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectedClientSnapshot {
    pub id: ClientId,
    pub identification: String,
    pub remote_addr: String,
    pub client_type: String,
    pub subscriptions: Vec<EventFilterSnapshot>,
    pub events_per_second: f32,
    pub uptime_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandwidthSnapshot {
    pub outbound_bytes_per_second: u64,
    pub outbound_bytes_total: u64,
    pub peak_outbound_bytes_per_second: u64,
}

#[derive(Debug, Clone, PartialEq)]
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
    let connected_clients = connected_clients_snapshot(bus_adapter).await;
    let aggregate_events_per_second = connected_clients.iter().map(|c| c.events_per_second).sum();
    let bw = &server_info.bandwidth;

    ServerSnapshot {
        uptime_seconds: server_info.uptime_seconds(),
        connected_clients,
        bandwidth: BandwidthSnapshot {
            outbound_bytes_per_second: bw.current_bytes_per_second(),
            outbound_bytes_total: bw.total(),
            peak_outbound_bytes_per_second: bw.peak_bytes_per_second(),
        },
        aggregate_events_per_second,
        http_requests_total: server_info.http_requests(),
        events_out_total: server_info.events_out(),
        dropped_events_total: server_info.dropped_events(),
    }
}

async fn connected_clients_snapshot(bus_adapter: &BusAdapter) -> Vec<ConnectedClientSnapshot> {
    bus_adapter
        .client_rows()
        .await
        .into_iter()
        .map(ConnectedClientSnapshot::from)
        .collect()
}

pub(crate) struct ClientRow {
    pub(crate) id: ClientId,
    pub(crate) identification: String,
    pub(crate) remote_addr: String,
    pub(crate) client_type: String,
    pub(crate) subscriptions: Vec<EventFilterSnapshot>,
    pub(crate) events_per_second: f32,
    pub(crate) uptime_seconds: i64,
    pub(crate) bytes_sent: u64,
}

impl ClientRow {
    pub(crate) fn new(id: ClientId, session: &WsClient, filters: &HashSet<EventFilter>) -> Self {
        Self {
            id,
            identification: session.identification.load_full().as_str().to_owned(),
            remote_addr: session.remote_addr.ip().to_string(),
            client_type: session.client_type.load_full().type_str().to_owned(),
            subscriptions: filters
                .iter()
                .map(|f| EventFilterSnapshot {
                    source: f.source,
                    kind: f.kind.clone(),
                })
                .collect(),
            events_per_second: session.events_per_second(),
            uptime_seconds: session.uptime().whole_seconds(),
            bytes_sent: session.bytes_sent_session.load(Ordering::Relaxed),
        }
    }
}

impl From<ClientRow> for ConnectedClientSnapshot {
    fn from(row: ClientRow) -> Self {
        Self {
            id: row.id,
            identification: row.identification,
            remote_addr: row.remote_addr,
            client_type: row.client_type,
            subscriptions: row.subscriptions,
            events_per_second: row.events_per_second,
            uptime_seconds: row.uptime_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use forge_runtime::{EventBus, NullEventLogRepo};

    use super::build_server_snapshot;
    use crate::delivery::bus_adapter::{BusAdapter, ClientFilterSet, OverlayClientClass};
    use crate::delivery::ws_client::WsClient;
    use crate::lifecycle::server_info::ServerInfo;

    #[tokio::test]
    async fn the_server_console_counts_the_browser_source_and_leaves_out_the_preview_tab() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let adapter = BusAdapter::new(bus);
        let info = ServerInfo::serving(std::collections::BTreeSet::new());
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

        let snapshot = build_server_snapshot(&info, &adapter).await;

        let counted: Vec<&str> = snapshot
            .connected_clients
            .iter()
            .map(|client| client.identification.as_str())
            .collect();
        assert_eq!(
            counted,
            vec!["browser-source"],
            "the console's connected-client count must not include a page the maintainer opened \
             to look at an overlay"
        );
    }
}
