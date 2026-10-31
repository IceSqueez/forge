use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use forge_types::PlatformId;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::bandwidth::BandwidthTracker;
use crate::bus_adapter::ClientId;
use crate::ws_client::WsClient;

pub(crate) struct ServerInfo {
    pub version: &'static str,
    pub started_at: OffsetDateTime,
    pub available_platforms: BTreeSet<PlatformId>,
    pub connected_clients: Arc<RwLock<HashMap<ClientId, Arc<WsClient>>>>,
    pub bandwidth: Arc<BandwidthTracker>,
    pub http_requests_total: AtomicU64,
    pub events_out_total: AtomicU64,
    pub dropped_events_total: AtomicU64,
}

impl ServerInfo {
    pub(crate) fn serving(available_platforms: BTreeSet<PlatformId>) -> Arc<Self> {
        Arc::new(Self {
            version: env!("CARGO_PKG_VERSION"),
            started_at: OffsetDateTime::now_utc(),
            available_platforms,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
            bandwidth: Arc::new(BandwidthTracker::new()),
            http_requests_total: AtomicU64::new(0),
            events_out_total: AtomicU64::new(0),
            dropped_events_total: AtomicU64::new(0),
        })
    }

    pub(crate) fn record_http_request(&self) {
        self.http_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_event_out(&self) {
        self.events_out_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_dropped_events(&self, count: u64) {
        self.dropped_events_total
            .fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn http_requests(&self) -> u64 {
        self.http_requests_total.load(Ordering::Relaxed)
    }

    pub(crate) fn events_out(&self) -> u64 {
        self.events_out_total.load(Ordering::Relaxed)
    }

    pub(crate) fn dropped_events(&self) -> u64 {
        self.dropped_events_total.load(Ordering::Relaxed)
    }

    pub(crate) fn uptime_seconds(&self) -> i64 {
        (OffsetDateTime::now_utc() - self.started_at).whole_seconds()
    }

    pub(crate) async fn register(&self, id: ClientId, client: Arc<WsClient>) {
        self.connected_clients.write().await.insert(id, client);
    }

    pub(crate) async fn unregister(&self, id: ClientId) {
        self.connected_clients.write().await.remove(&id);
    }
}
