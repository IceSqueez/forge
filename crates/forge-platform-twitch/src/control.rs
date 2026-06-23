use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome, PlatformError};

use crate::auth::fetch_user_info;
use crate::builtin::TwitchIntegrationBundle;
use crate::credentials::load;

#[async_trait]
impl BuiltinControl for TwitchIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        let stored = load(self.credentials().as_ref())
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        // Tear down the current session before spawning a fresh one: the old
        // handle consumes itself on shutdown, so take() it out of the slot first.
        if let Some(old) = self.handle_slot().lock().await.take() {
            old.shutdown();
        }

        let handle = self.spawn_chat(stored.access_token);
        *self.handle_slot().lock().await = Some(handle);
        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let handle = self.handle_slot().lock().await.take();
        match handle {
            Some(h) => {
                h.shutdown();
                Ok(())
            }
            None => Err(ControlFailure::NotConnected),
        }
    }

    async fn refresh_token(&self) -> ControlOutcome {
        let stored = load(self.credentials().as_ref())
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        match fetch_user_info(&stored.access_token, &self.config().client_id).await {
            Ok(_) => Ok(()),
            Err(PlatformError::Auth { .. }) => Err(ControlFailure::Unauthorized),
            Err(PlatformError::Http { status: 401, .. }) => Err(ControlFailure::Unauthorized),
            Err(_) => Err(ControlFailure::Transport),
        }
    }
}
