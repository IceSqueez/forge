use async_trait::async_trait;
use forge_runtime::OverlayFrameSink;
use forge_server::ServerHandle;
use forge_storage::OverlayId;

pub struct ServerOverlayFrameSink {
    server: ServerHandle,
}

impl ServerOverlayFrameSink {
    pub fn new(server: ServerHandle) -> Self {
        Self { server }
    }
}

#[async_trait]
impl OverlayFrameSink for ServerOverlayFrameSink {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> usize {
        self.server
            .deliver_overlay_content(identity, content, duration_ms)
            .await
    }

    async fn deliver_reload(&self, identity: &OverlayId) {
        self.server.deliver_overlay_reload(Some(identity)).await;
    }

    async fn revoke(&self, identity: &OverlayId) {
        self.server.revoke_overlay(identity).await;
    }
}
