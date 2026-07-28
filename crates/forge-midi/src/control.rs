use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome};

use crate::client::MidiClient;

#[async_trait]
impl BuiltinControl for MidiClient {
    async fn reconnect(&self) -> ControlOutcome {
        self.enable_input()
            .await
            .map_err(|_| ControlFailure::Transport)
    }

    async fn disconnect(&self) -> ControlOutcome {
        self.disable_input()
            .await
            .map_err(|_| ControlFailure::Transport)
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}
