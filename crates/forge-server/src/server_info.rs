use std::collections::HashMap;
use std::sync::Arc;

use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::bandwidth::BandwidthTracker;
use crate::bus_adapter::ClientId;
use crate::ws_client::WsClient;

pub struct ServerInfo {
    pub version: &'static str,
    pub started_at: OffsetDateTime,
    pub connected_clients: Arc<RwLock<HashMap<ClientId, Arc<WsClient>>>>,
    pub bandwidth: Arc<BandwidthTracker>,
}

impl ServerInfo {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            version: env!("CARGO_PKG_VERSION"),
            started_at: OffsetDateTime::now_utc(),
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
            bandwidth: Arc::new(BandwidthTracker::new()),
        })
    }

    pub fn uptime_seconds(&self) -> i64 {
        (OffsetDateTime::now_utc() - self.started_at).whole_seconds()
    }

    pub async fn register(&self, id: ClientId, client: Arc<WsClient>) {
        self.connected_clients.write().await.insert(id, client);
    }

    pub async fn unregister(&self, id: ClientId) {
        self.connected_clients.write().await.remove(&id);
    }
}
