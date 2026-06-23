use async_trait::async_trait;
use forge_platform_core::{BuiltinControl, ControlFailure, ControlOutcome, PlatformError};

use crate::builtin::KickIntegrationBundle;
use crate::chat::KickChat;

#[async_trait]
impl BuiltinControl for KickIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        let creds = self
            .credentials_manager()
            .load()
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        // Take the old handle out of the slot and drop it outside the lock.
        // Dropping close_tx signals the WS run-loop to exit; no explicit join
        // needed — the task runs to completion independently.
        let old = {
            let mut slot = self.handle_slot().lock().await;
            slot.take()
        };
        drop(old);

        let chat = KickChat::new(self.slug().to_owned(), self.http().clone());
        let new_handle = chat
            .connect(self.event_tx().clone())
            .await
            .map_err(|_| ControlFailure::Transport)?;

        {
            let mut slot = self.handle_slot().lock().await;
            *slot = Some(new_handle);
        }

        let _ = creds; // consumed for credentials check; token stays inside the crate
        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let handle = {
            let mut slot = self.handle_slot().lock().await;
            slot.take()
        };
        match handle {
            Some(_h) => {
                // Dropping close_tx signals the run-loop to stop.
                Ok(())
            }
            None => Err(ControlFailure::NotConnected),
        }
    }

    async fn refresh_token(&self) -> ControlOutcome {
        let creds = self
            .credentials_manager()
            .load()
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        match self
            .credentials_manager()
            .refresh(&creds.refresh_token)
            .await
        {
            Ok(_) => Ok(()),
            Err(PlatformError::ReauthRequired { .. }) => Err(ControlFailure::Unauthorized),
            Err(PlatformError::Http { status: 401, .. }) => Err(ControlFailure::Unauthorized),
            Err(PlatformError::Auth { .. }) => Err(ControlFailure::Unauthorized),
            Err(_) => Err(ControlFailure::Transport),
        }
    }
}
