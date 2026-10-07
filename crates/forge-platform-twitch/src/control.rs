use async_trait::async_trait;
use forge_platform_core::{
    BuiltinControl, ChatPlatform, ControlFailure, ControlOutcome, PlatformError,
};

use crate::builtin::TwitchIntegrationBundle;
use crate::credentials::load;

#[async_trait]
impl BuiltinControl for TwitchIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        load(self.credentials().as_ref())
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        self.platform().connect().await.map_err(|e| match e {
            PlatformError::ReauthRequired { .. } => ControlFailure::NotConnected,
            _ => ControlFailure::Transport,
        })
    }

    async fn disconnect(&self) -> ControlOutcome {
        self.platform()
            .disconnect()
            .await
            .map_err(|_| ControlFailure::Transport)
    }

    async fn refresh_token(&self) -> ControlOutcome {
        let stored = load(self.credentials().as_ref())
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        if stored.refresh_token.is_none() {
            return Err(ControlFailure::Unauthorized);
        }

        match self
            .credentials_manager()
            .refresh(&stored.access_token)
            .await
        {
            Ok(_) => {
                self.refresh_identity().await;
                Ok(())
            }
            Err(PlatformError::ReauthRequired { .. }) => Err(ControlFailure::Unauthorized),
            Err(_) => Err(ControlFailure::Transport),
        }
    }
}
