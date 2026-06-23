use std::sync::Arc;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use tokio::sync::Notify;

use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome};

use crate::client::{STATE_CONNECTING, VTubeClient};
use crate::supervisor::SupervisorContext;

#[async_trait]
impl BuiltinControl for VTubeClient {
    async fn reconnect(&self) -> ControlOutcome {
        // Serialise concurrent reconnect/disconnect calls: only one supervisor
        // replacement runs at a time.  Hold the slot only long enough to swap
        // the Notify; release it before awaiting the old handle so the
        // supervisor's own shutdown.notified() path can complete without
        // waiting on us.
        let new_notify = Arc::new(Notify::new());
        {
            let mut slot = self.shutdown.lock().await;
            let old_notify = slot.clone();
            *slot = Arc::clone(&new_notify);
            old_notify.notify_one();
        }

        let old_handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = old_handle {
            let _ = h.await;
        }

        self.state.store(STATE_CONNECTING, Ordering::Release);
        if let Ok(mut g) = self.connected_at.write() {
            *g = None;
        }

        // Fresh request channel — the previous req_rx is gone with the old supervisor.
        let (new_req_tx, new_req_rx) = tokio::sync::mpsc::unbounded_channel();
        {
            let mut tx_slot = self.req_tx.lock().await;
            *tx_slot = new_req_tx;
        }

        let ctx = SupervisorContext {
            endpoint: self.config.endpoint.clone(),
            state: Arc::clone(&self.state),
            auth_state: Arc::clone(&self.auth_state),
            shutdown: new_notify,
            connected_at: Arc::clone(&self.connected_at),
            publisher: Arc::clone(&self.reconnect_publisher),
            creds: Arc::clone(&self.reconnect_creds),
            req_rx: new_req_rx,
            health_state: Arc::clone(&self.health_state),
            health_tx: self.health_tx.clone(),
            content_notifier: self.content_notifier.clone(),
        };
        let new_handle = tokio::spawn(crate::supervisor::run_supervisor(ctx));
        if let Ok(mut g) = self.supervisor.lock() {
            *g = Some(new_handle);
        }

        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let slot = self.shutdown.lock().await;
        let notify = slot.clone();
        drop(slot);
        notify.notify_one();

        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }

        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}
