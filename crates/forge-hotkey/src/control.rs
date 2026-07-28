use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome};

use crate::client::HotkeyClient;

#[async_trait]
impl BuiltinControl for HotkeyClient {
    async fn reconnect(&self) -> ControlOutcome {
        self.enable()
            .await
            .map(|_| ())
            .map_err(|_| ControlFailure::Transport)
    }

    async fn disconnect(&self) -> ControlOutcome {
        self.disable().await.map_err(|_| ControlFailure::Transport)
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::backend::tests::MockPortalBackend;
    use crate::client::tests::{noop_publisher, start_supervised};
    use crate::combo::HotkeyCombo;

    #[tokio::test]
    async fn disconnect_disables_the_engine_and_reconnect_restores_it() {
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, noop_publisher());
        client
            .register(HotkeyCombo::parse("Ctrl+F1").unwrap())
            .await
            .unwrap();
        let control: Arc<dyn BuiltinControl> = Arc::clone(&client) as Arc<dyn BuiltinControl>;

        let mut health_rx = client.health_tx.subscribe();
        control.disconnect().await.unwrap();
        health_rx.recv().await.unwrap();
        assert!(!client.is_enabled());

        control.reconnect().await.unwrap();
        assert!(client.is_enabled());
    }

    #[tokio::test]
    async fn control_calls_report_transport_when_no_supervisor_runs() {
        let client = crate::client::HotkeyClient::new_for_test(None);
        let control: &dyn BuiltinControl = &*client;

        assert_eq!(control.disconnect().await, Err(ControlFailure::Transport));
        assert_eq!(control.reconnect().await, Err(ControlFailure::Transport));
    }
}
