use async_trait::async_trait;
use forge_platform_core::{
    BuiltinControl, ChatPlatform, ConnectionState, ControlFailure, ControlOutcome, PlatformError,
};

use crate::builtin::KickIntegrationBundle;

#[async_trait]
impl BuiltinControl for KickIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        self.credentials_manager()
            .load()
            .await
            .map_err(|_| ControlFailure::Transport)?
            .ok_or(ControlFailure::NotConnected)?;

        self.platform()
            .connect()
            .await
            .map_err(map_transport_failure)?;
        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        if matches!(
            self.platform().connection_state(),
            ConnectionState::Disconnected
        ) {
            return Err(ControlFailure::NotConnected);
        }
        self.platform()
            .disconnect()
            .await
            .map_err(map_transport_failure)?;
        Ok(())
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

fn map_transport_failure(err: PlatformError) -> ControlFailure {
    match err {
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. } => {
            ControlFailure::Unauthorized
        }
        _ => ControlFailure::Transport,
    }
}
