use std::sync::Arc;

use tokio::sync::watch;

#[derive(Debug, Clone)]
pub(crate) struct OverlayDefinitionRevision(Arc<watch::Sender<u64>>);

impl Default for OverlayDefinitionRevision {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(0)))
    }
}

impl OverlayDefinitionRevision {
    pub(crate) fn advance(&self) {
        self.0
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub(crate) fn subscribe(&self) -> OverlayDefinitionChanges {
        OverlayDefinitionChanges(self.0.subscribe())
    }
}

pub struct OverlayDefinitionChanges(watch::Receiver<u64>);

impl OverlayDefinitionChanges {
    pub async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }
}
