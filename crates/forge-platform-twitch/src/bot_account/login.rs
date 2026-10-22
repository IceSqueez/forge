use forge_platform_core::PlatformError;
use forge_storage::CredentialId;
use forge_types::OAuthToken;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use super::{BotStatus, TwitchBotAccount};
use crate::auth::{
    DEVICE_CANCELLED_REASON, DEVICE_CODE_USED_REASON, DEVICE_DENIED_REASON, DEVICE_EXPIRED_REASON,
    DeviceCodeInfo, TwitchAuthBundle, TwitchAuthFlow, twitch_revoke_endpoint,
};
use crate::credentials::{TWITCH_BOT_CREDENTIAL_ID, load, load_at, store_at};
use crate::helix::bounded_http_client;

#[derive(Debug, Error)]
pub enum BotLoginError {
    #[error("this is your streamer account; sign in as the bot account instead")]
    SameAsBroadcaster,
    #[error("bot sign-in was not started")]
    NotStarted,
    #[error("bot sign-in cancelled")]
    Cancelled,
    #[error("bot sign-in denied")]
    Denied,
    #[error("bot sign-in code expired")]
    Expired,
    #[error(transparent)]
    Platform(PlatformError),
    #[error("credential storage failed: {0}")]
    Storage(String),
}

impl From<PlatformError> for BotLoginError {
    fn from(error: PlatformError) -> Self {
        let PlatformError::Auth { reason } = &error else {
            return Self::Platform(error);
        };
        match reason.as_str() {
            DEVICE_CANCELLED_REASON => Self::Cancelled,
            DEVICE_DENIED_REASON => Self::Denied,
            DEVICE_EXPIRED_REASON | DEVICE_CODE_USED_REASON => Self::Expired,
            _ => Self::Platform(error),
        }
    }
}

impl TwitchBotAccount {
    pub async fn begin_login(&self) -> Result<DeviceCodeInfo, BotLoginError> {
        self.cancel_login();
        let mut flow = TwitchAuthFlow::for_bot(&self.endpoints, self.client_id.clone());
        let info = flow.start().await?;
        *self.pending_flow.lock().await = Some(flow);
        Ok(info)
    }

    pub async fn await_login(&self) -> Result<BotStatus, BotLoginError> {
        let mut flow = self
            .pending_flow
            .lock()
            .await
            .take()
            .ok_or(BotLoginError::NotStarted)?;
        let cancel = self
            .login_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let bundle = flow.wait_for_authorization(cancel).await?;
        self.adopt_grant(bundle).await
    }

    pub(crate) async fn adopt_grant(
        &self,
        bundle: TwitchAuthBundle,
    ) -> Result<BotStatus, BotLoginError> {
        let broadcaster = load(self.repo.as_ref())
            .await
            .map_err(|e| BotLoginError::Storage(e.to_string()))?;
        if broadcaster.is_some_and(|stored| stored.user_id == bundle.user_info.id) {
            self.revoke_pair(&bundle.access_token, bundle.refresh_token.as_ref())
                .await;
            return Err(BotLoginError::SameAsBroadcaster);
        }

        store_at(self.repo.as_ref(), TWITCH_BOT_CREDENTIAL_ID, &bundle)
            .await
            .map_err(|e| BotLoginError::Storage(e.to_string()))?;
        let status = BotStatus::Connected {
            login: bundle.user_info.login,
        };
        self.set_state(status.clone(), Some(bundle.user_info.id));
        Ok(status)
    }

    pub fn cancel_login(&self) {
        let mut guard = self.login_cancel.lock().unwrap_or_else(|p| p.into_inner());
        guard.cancel();
        *guard = CancellationToken::new();
    }

    pub async fn logout(&self) -> Result<(), BotLoginError> {
        self.cancel_login();
        *self.pending_flow.lock().await = None;
        let stored = load_at(self.repo.as_ref(), TWITCH_BOT_CREDENTIAL_ID)
            .await
            .map_err(|e| BotLoginError::Storage(e.to_string()))?;
        if let Some(stored) = stored {
            self.revoke_pair(&stored.access_token, stored.refresh_token.as_ref())
                .await;
        }
        self.repo
            .delete(&CredentialId::new(TWITCH_BOT_CREDENTIAL_ID))
            .await
            .map_err(|e| BotLoginError::Storage(e.to_string()))?;
        self.set_state(BotStatus::NotConnected, None);
        Ok(())
    }

    async fn revoke_pair(&self, access: &OAuthToken, refresh: Option<&OAuthToken>) {
        self.revoke(access).await;
        if let Some(refresh) = refresh {
            self.revoke(refresh).await;
        }
    }

    async fn revoke(&self, token: &OAuthToken) {
        let form = [
            ("client_id", self.client_id.as_str()),
            ("token", token.expose()),
        ];
        match bounded_http_client()
            .post(twitch_revoke_endpoint(&self.endpoints))
            .form(&form)
            .send()
            .await
        {
            Ok(response) => debug!(status = response.status().as_u16(), "bot token revoke sent"),
            Err(e) => debug!(error = %e.without_url(), "bot token revoke failed"),
        }
    }
}
