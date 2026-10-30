use async_trait::async_trait;
use forge_platform_core::{
    BuiltinControl, ChatPlatform, ControlFailure, ControlOutcome, PlatformError,
};
use forge_storage::StorageError;
use tracing::warn;

use crate::builtin::TwitchIntegrationBundle;
use crate::credentials::load;

#[async_trait]
impl BuiltinControl for TwitchIntegrationBundle {
    async fn reconnect(&self) -> ControlOutcome {
        load(self.credentials().as_ref())
            .await
            .map_err(credential_load_failure)?
            .ok_or(ControlFailure::NotConnected)?;

        self.platform().connect().await.map_err(|e| match e {
            PlatformError::ReauthRequired { .. } => ControlFailure::NotConnected,
            other => {
                warn!(error = %other, "twitch reconnect failed");
                ControlFailure::Transport
            }
        })
    }

    async fn disconnect(&self) -> ControlOutcome {
        self.platform().disconnect().await.map_err(|e| {
            warn!(error = %e, "twitch disconnect failed");
            ControlFailure::Transport
        })
    }

    async fn refresh_token(&self) -> ControlOutcome {
        let stored = load(self.credentials().as_ref())
            .await
            .map_err(credential_load_failure)?
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
            Err(e) => {
                warn!(error = %e, "twitch token refresh failed");
                Err(ControlFailure::Transport)
            }
        }
    }
}

fn credential_load_failure(e: StorageError) -> ControlFailure {
    warn!(error = %e, "twitch credentials could not be loaded");
    if e.stored_secret_unrecoverable() {
        ControlFailure::Unauthorized
    } else {
        ControlFailure::Transport
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_platform_core::{BuiltinControl, ControlFailure};
    use forge_storage::StorageError;
    use tokio::sync::watch;

    use crate::account::identity::BroadcasterTier;
    use crate::bot_account::test_support::KeyedRepo;
    use crate::builtin::TwitchIntegrationBundle;
    use crate::chat::ChatConnectionState;
    use crate::subscriptions::SubscriptionTracker;

    fn undecryptable() -> StorageError {
        StorageError::Decryption
    }

    fn offline() -> StorageError {
        StorageError::Connection {
            reason: "offline".to_owned(),
        }
    }

    fn bundle_over(repo: Arc<KeyedRepo>) -> Arc<TwitchIntegrationBundle> {
        let (_tx, rx) = watch::channel(ChatConnectionState::Disconnected);
        TwitchIntegrationBundle::for_test(
            Some("streamer".to_owned()),
            rx,
            SubscriptionTracker::default(),
            repo,
            BroadcasterTier::Standard,
        )
    }

    #[tokio::test]
    async fn a_credential_load_failure_asks_for_sign_in_only_when_the_secret_is_lost() {
        let cases: [(fn() -> StorageError, ControlFailure); 2] = [
            (undecryptable, ControlFailure::Unauthorized),
            (offline, ControlFailure::Transport),
        ];
        for (error, want) in cases {
            let repo = Arc::new(KeyedRepo::default());
            repo.fail_loads_with(error);
            let bundle = bundle_over(repo);

            assert_eq!(bundle.reconnect().await, Err(want.clone()));
            assert_eq!(bundle.refresh_token().await, Err(want));
        }
    }

    #[tokio::test]
    async fn a_missing_credential_is_not_connected_at_the_control_boundary() {
        let bundle = bundle_over(Arc::new(KeyedRepo::default()));

        assert_eq!(bundle.reconnect().await, Err(ControlFailure::NotConnected));
        assert_eq!(
            bundle.refresh_token().await,
            Err(ControlFailure::NotConnected)
        );
    }
}
