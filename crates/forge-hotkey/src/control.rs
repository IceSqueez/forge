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
