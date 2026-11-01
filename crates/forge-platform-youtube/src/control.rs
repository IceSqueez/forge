use async_trait::async_trait;
use forge_platform_core::{
    BuiltinControl, ChatPlatform, ControlOutcome, CredentialStateTracker, CredentialedChatBundle,
    PlatformError, disconnect_credentialed, reconnect_credentialed, refresh_credentialed,
};

use crate::builtin::YouTubeIntegrationBundle;

#[async_trait]
impl BuiltinControl for YouTubeIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        reconnect_credentialed(self).await
    }

    async fn disconnect(&self) -> ControlOutcome {
        disconnect_credentialed(self).await
    }

    async fn refresh_token(&self) -> ControlOutcome {
        refresh_credentialed(self).await
    }
}

#[async_trait]
impl CredentialedChatBundle for YouTubeIntegrationBundle {
    fn chat_platform(&self) -> &dyn ChatPlatform {
        self.platform().as_ref()
    }

    fn credential_state(&self) -> &CredentialStateTracker {
        self.credentials_manager().credential_state()
    }

    async fn has_stored_credentials(&self) -> Result<bool, PlatformError> {
        Ok(self.credentials_manager().load().await?.is_some())
    }

    async fn refresh_if_stored(&self) -> Result<bool, PlatformError> {
        let manager = self.credentials_manager();
        let Some(stored) = manager.load().await? else {
            return Ok(false);
        };
        manager.refresh(stored.access_token.expose()).await?;
        Ok(true)
    }
}
