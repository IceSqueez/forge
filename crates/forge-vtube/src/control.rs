use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Notify;

use forge_platform_core::{BuiltinControl, ConnectionState, ControlFailure, ControlOutcome};

use crate::client::VTubeClient;
use crate::supervisor::SupervisorContext;

#[async_trait]
impl BuiltinControl for VTubeClient {
    async fn reconnect(&self) -> ControlOutcome {
        // Slot held only long enough to swap the Notify; released before awaiting the old
        // handle so the supervisor's own shutdown.notified() path can complete.
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

        self.state.store(ConnectionState::Connecting);
        if let Ok(mut g) = self.connected_at.write() {
            *g = None;
        }

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_platform_core::{BuiltinControl, ControlFailure};

    use crate::client::VTubeClient;

    #[test]
    fn client_coerces_to_dyn_builtin_control() {
        fn accepts(_: Arc<dyn BuiltinControl>) {}
        accepts(Arc::new(VTubeClient::new_for_test("ws://127.0.0.1:8001/")));
    }

    #[tokio::test]
    async fn refresh_token_is_unsupported() {
        let client = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let outcome = client.refresh_token().await;
        assert_eq!(outcome, Err(ControlFailure::Unsupported));
    }
}
