use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome, PlatformError};

use crate::builtin::TwitchIntegrationBundle;
use crate::credentials::load;

#[async_trait]
impl BuiltinControl for TwitchIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        load(self.credentials().as_ref())
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        let mut slot = self.handle_slot().lock().await;
        if let Some(old) = slot.take() {
            old.shutdown().await;
        }
        *slot = Some(self.spawn_chat());
        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let mut slot = self.handle_slot().lock().await;
        if let Some(handle) = slot.take() {
            handle.shutdown().await;
        }
        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        let stored = load(self.credentials().as_ref())
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        // No stored refresh token means renewal is impossible - surface the
        // re-auth prompt rather than pretending the token was renewed.
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
