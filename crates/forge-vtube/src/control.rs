use forge_types::RecoverRwLock;

use async_trait::async_trait;

use forge_platform_core::{
    BuiltinControl, ConnectionState, ConnectionTransition, ControlFailure, ControlOutcome,
    record_connection_failure,
};

use crate::client::VTubeClient;

#[async_trait]
impl BuiltinControl for VTubeClient {
    async fn reconnect(&self) -> ControlOutcome {
        let _lifecycle = self.supervision.lifecycle.lock().await;
        let new_notify = self.supervision.stop_for_restart().await;

        crate::supervisor::set_connection_state(
            &self.state,
            &*self.supervision.publisher,
            ConnectionState::Connecting,
            ConnectionTransition::Expected,
        );
        *self.connected_at.write_recover() = None;
        record_connection_failure(&self.state, &self.last_failure, None);

        let (new_req_tx, new_req_rx) = tokio::sync::mpsc::unbounded_channel();
        *self.req_tx.lock().await = new_req_tx;

        self.launch_supervisor(new_notify, new_req_rx);

        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let _lifecycle = self.supervision.lifecycle.lock().await;
        self.supervision.stop().await;
        record_connection_failure(&self.state, &self.last_failure, None);
        crate::supervisor::set_connection_state(
            &self.state,
            &*self.supervision.publisher,
            ConnectionState::Disconnected,
            ConnectionTransition::Expected,
        );

        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::{BuiltinControl, ControlFailure};

    use crate::client::VTubeClient;

    #[tokio::test]
    async fn refresh_token_is_unsupported() {
        let client = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let outcome = client.refresh_token().await;
        assert_eq!(outcome, Err(ControlFailure::Unsupported));
    }
}
