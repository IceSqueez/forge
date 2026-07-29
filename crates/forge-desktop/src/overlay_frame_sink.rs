use async_trait::async_trait;
use forge_events::Event;
use forge_runtime::OverlayFrameSink;
use forge_server::ServerHandle;

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
    async fn deliver(&self, event: Event) {
        self.server.deliver_event(event).await;
    }
}
