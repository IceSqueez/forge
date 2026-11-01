use async_trait::async_trait;

use crate::builtin::{ControlFailure, ControlOutcome};
use crate::chat::{ChatPlatform, ConnectionState};
use crate::credential_state::{CredentialState, CredentialStateTracker};
use crate::error::PlatformError;

#[async_trait]
pub trait CredentialedChatBundle: Send + Sync {
    fn chat_platform(&self) -> &dyn ChatPlatform;

    fn credential_state(&self) -> &CredentialStateTracker;

    async fn has_stored_credentials(&self) -> Result<bool, PlatformError>;

    async fn refresh_if_stored(&self) -> Result<bool, PlatformError>;
}

pub async fn reconnect_credentialed(bundle: &dyn CredentialedChatBundle) -> ControlOutcome {
    let stored = bundle
        .has_stored_credentials()
        .await
        .map_err(credential_load_failure)?;
    if !stored {
        return Err(ControlFailure::NotConnected);
    }
    bundle
        .chat_platform()
        .connect()
        .await
        .map_err(connection_failure)
}

pub async fn disconnect_credentialed(bundle: &dyn CredentialedChatBundle) -> ControlOutcome {
    if matches!(
        bundle.chat_platform().connection_state(),
        ConnectionState::Disconnected
    ) {
        return Err(ControlFailure::NotConnected);
    }
    bundle
        .chat_platform()
        .disconnect()
        .await
        .map_err(connection_failure)
}

pub async fn refresh_credentialed(bundle: &dyn CredentialedChatBundle) -> ControlOutcome {
    let outcome = match bundle.refresh_if_stored().await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ControlFailure::NotConnected),
        Err(error) => Err(refresh_failure(&error)),
    };
    if matches!(outcome, Err(ControlFailure::Unauthorized)) {
        bundle
            .credential_state()
            .report(CredentialState::ReauthRequired);
    }
    outcome
}

fn refresh_failure(error: &PlatformError) -> ControlFailure {
    match error {
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. } => {
            ControlFailure::Unauthorized
        }
        PlatformError::Http {
            status: forge_net::status::UNAUTHORIZED,
            ..
        } => ControlFailure::Unauthorized,
        _ => ControlFailure::Transport,
    }
}

fn credential_load_failure(error: PlatformError) -> ControlFailure {
    tracing::warn!(%error, "control could not load stored credentials");
    ControlFailure::Transport
}

fn connection_failure(error: PlatformError) -> ControlFailure {
    match error {
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. } => {
            ControlFailure::Unauthorized
        }
        _ => ControlFailure::Transport,
    }
}
