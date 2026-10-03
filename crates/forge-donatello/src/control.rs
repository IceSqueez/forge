use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome};

use crate::provider::DonatelloProvider;

#[async_trait]
impl BuiltinControl for DonatelloProvider {
    async fn reconnect(&self) -> ControlOutcome {
        self.restart();
        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        self.pause();
        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}
