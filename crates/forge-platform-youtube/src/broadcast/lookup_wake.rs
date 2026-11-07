use std::sync::Arc;

use tokio::sync::watch;

#[derive(Clone)]
pub struct BroadcastLookupWake(Arc<watch::Sender<u64>>);

impl BroadcastLookupWake {
    pub fn new() -> Self {
        Self(Arc::new(watch::Sender::new(0)))
    }

    pub fn wake(&self) {
        self.0
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub(crate) fn listen(&self) -> LookupWakeListener {
        LookupWakeListener(self.0.subscribe())
    }
}

impl Default for BroadcastLookupWake {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct LookupWakeListener(watch::Receiver<u64>);

impl LookupWakeListener {
    pub(crate) async fn woken(&mut self) {
        if self.0.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    pub(crate) fn discard_pending(&mut self) {
        self.0.mark_unchanged();
    }
}
