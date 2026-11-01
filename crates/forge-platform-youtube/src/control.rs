use async_trait::async_trait;
use forge_platform_core::{
    BuiltinControl, ChatPlatform, ConnectionState, ControlFailure, ControlOutcome, CredentialState,
    PlatformError,
};

use crate::builtin::YoutubeIntegrationBundle;

#[async_trait]
impl BuiltinControl for YoutubeIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        self.credentials_manager()
            .load()
            .await
            .map_err(credential_load_failure)?
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
        let outcome = self.refresh_stored_token().await;
        if matches!(outcome, Err(ControlFailure::Unauthorized)) {
            self.credentials_manager()
                .credential_state()
                .report(CredentialState::ReauthRequired);
        }
        outcome
    }
}

impl YoutubeIntegrationBundle {
    async fn refresh_stored_token(&self) -> ControlOutcome {
        let creds = self
            .credentials_manager()
            .load()
            .await
            .map_err(credential_load_failure)?
            .ok_or(ControlFailure::NotConnected)?;

        match self
            .credentials_manager()
            .refresh(creds.access_token.expose())
            .await
        {
            Ok(_) => Ok(()),
            Err(PlatformError::ReauthRequired { .. }) => Err(ControlFailure::Unauthorized),
            Err(PlatformError::Http {
                status: forge_net::status::UNAUTHORIZED,
                ..
            }) => Err(ControlFailure::Unauthorized),
            Err(PlatformError::Auth { .. }) => Err(ControlFailure::Unauthorized),
            Err(_) => Err(ControlFailure::Transport),
        }
    }
}

fn credential_load_failure(err: PlatformError) -> ControlFailure {
    tracing::warn!(error = %err, "control could not load stored credentials");
    ControlFailure::Transport
}

fn map_transport_failure(err: PlatformError) -> ControlFailure {
    match err {
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. } => {
            ControlFailure::Unauthorized
        }
        _ => ControlFailure::Transport,
    }
}
