use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome};

use crate::client::HotkeyClient;

#[async_trait]
impl BuiltinControl for HotkeyClient {
    async fn reconnect(&self) -> ControlOutcome {
        self.enable()
            .await
            .map(|_refused_combos| ())
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

        control.disconnect().await.unwrap();
        assert!(!client.is_enabled());

        control.reconnect().await.unwrap();
        assert!(client.is_enabled());
    }

    #[tokio::test]
    async fn reconnect_succeeds_and_re_enables_the_engine_when_one_combo_is_refused() {
        let (backend, _tx) = MockPortalBackend::new();
        let refuse = Arc::clone(&backend.fail_on);
        let client = start_supervised(backend, noop_publisher());
        client
            .register(HotkeyCombo::parse("Alt+X").unwrap())
            .await
            .unwrap();
        client
            .register(HotkeyCombo::parse("Ctrl+F1").unwrap())
            .await
            .unwrap();
        let control: &dyn BuiltinControl = &*client;

        control.disconnect().await.unwrap();
        refuse.lock().unwrap().insert("Alt+X".to_owned());

        assert_eq!(
            (control.reconnect().await, client.is_enabled()),
            (Ok(()), true)
        );
    }

    #[tokio::test]
    async fn control_calls_report_transport_when_no_supervisor_runs() {
        let client = crate::client::HotkeyClient::new_for_test(None);
        let control: &dyn BuiltinControl = &*client;

        assert_eq!(control.disconnect().await, Err(ControlFailure::Transport));
        assert_eq!(control.reconnect().await, Err(ControlFailure::Transport));
    }
}
