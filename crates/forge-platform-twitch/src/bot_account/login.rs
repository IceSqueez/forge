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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use wiremock::ResponseTemplate;

    use super::*;
    use crate::bot_account::BOT_STATUS_CHANGED_KIND;
    use crate::bot_account::test_support::{
        BOT_ACCESS, BOT_LOGIN, BOT_REFRESH, BOT_USER_ID, BotHarness,
    };
    use crate::credentials::TWITCH_CREDENTIAL_ID;
    use crate::sub_actions::test_support::{SELF_USER_ID, TOKEN_SENTINEL};

    async fn revoked_tokens(harness: &BotHarness) -> Vec<String> {
        harness
            .requests_to("/revoke")
            .await
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }

    #[tokio::test]
    async fn logout_revokes_both_bot_tokens_and_deletes_only_the_bot_row() {
        let harness = BotHarness::connected().await;
        harness
            .mount("POST", "/revoke", ResponseTemplate::new(200))
            .await;
        let broadcaster_before = harness.repo.raw(TWITCH_CREDENTIAL_ID);

        harness.bot.logout().await.unwrap();

        assert!(harness.repo.raw(TWITCH_BOT_CREDENTIAL_ID).is_none());
        assert_eq!(harness.repo.raw(TWITCH_CREDENTIAL_ID), broadcaster_before);
        let revoked = revoked_tokens(&harness).await;
        assert_eq!(revoked.len(), 2, "access and refresh token: {revoked:?}");
        assert!(revoked.iter().any(|body| body.contains(BOT_ACCESS)));
        assert!(revoked.iter().any(|body| body.contains(BOT_REFRESH)));
        assert!(
            revoked.iter().all(|body| !body.contains(TOKEN_SENTINEL)),
            "the streamer token must never be revoked by a bot logout"
        );
    }

    #[tokio::test]
    async fn logout_still_forgets_the_bot_when_twitch_cannot_revoke() {
        let harness = BotHarness::connected().await;
        harness
            .mount("POST", "/revoke", ResponseTemplate::new(503))
            .await;

        harness.bot.logout().await.unwrap();

        assert!(harness.repo.raw(TWITCH_BOT_CREDENTIAL_ID).is_none());
        assert_eq!(harness.bot.status(), BotStatus::NotConnected);
        assert!(harness.bot.connected_user_id().is_none());
        let last_status = harness
            .bus
            .of_kind(BOT_STATUS_CHANGED_KIND)
            .pop()
            .unwrap()
            .payload;
        assert_eq!(
            last_status,
            serde_json::json!({"status": "not_connected", "login": null})
        );
    }

    #[tokio::test]
    async fn await_login_before_begin_login_reports_not_started() {
        let harness = BotHarness::without_bot_row().await;

        let result = harness.bot.await_login().await;

        assert!(matches!(result, Err(BotLoginError::NotStarted)));
    }

    #[tokio::test]
    async fn begin_login_requests_exactly_the_bot_scopes() {
        let harness = BotHarness::without_bot_row().await;
        harness.mount("POST", "/device", device_response()).await;

        harness.bot.begin_login().await.unwrap();

        let body = String::from_utf8_lossy(&harness.requests_to("/device").await[0].body)
            .replace("%3A", ":")
            .replace('+', " ");
        let requested = body
            .split('&')
            .find_map(|pair| pair.strip_prefix("scopes="))
            .unwrap()
            .to_owned();
        assert_eq!(requested, crate::auth::TWITCH_BOT_SCOPES.join(" "));
    }

    fn device_response() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "BOT_DEVICE",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://www.twitch.tv/activate",
            "expires_in": 1800,
            "interval": 5,
        }))
    }

    #[tokio::test]
    async fn cancel_login_aborts_a_pending_bot_login_without_polling_or_storing() {
        let harness = BotHarness::without_bot_row().await;
        harness.mount("POST", "/device", device_response()).await;
        harness.bot.begin_login().await.unwrap();
        let bot = Arc::clone(&harness.bot);
        let pending = tokio::spawn(async move { bot.await_login().await });
        tokio::task::yield_now().await;

        harness.bot.cancel_login();
        let result = pending.await.unwrap();

        assert!(
            matches!(result, Err(BotLoginError::Cancelled)),
            "got {result:?}"
        );
        assert!(harness.requests_to("/token").await.is_empty());
        assert!(harness.repo.raw(TWITCH_BOT_CREDENTIAL_ID).is_none());
    }

    fn grant_for(user_id: &str, login: &str) -> TwitchAuthBundle {
        TwitchAuthBundle {
            access_token: OAuthToken::new("GRANT_ACCESS_SENTINEL_p3"),
            refresh_token: Some(OAuthToken::new("GRANT_REFRESH_SENTINEL_p3")),
            user_info: crate::auth::UserInfo {
                id: user_id.to_owned(),
                login: login.to_owned(),
                display_name: login.to_owned(),
                broadcaster_type: Default::default(),
            },
            client_id: crate::bot_account::test_support::CLIENT_ID.to_owned(),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn adopting_a_grant_for_the_streamer_account_revokes_it_and_stores_nothing() {
        let harness = BotHarness::without_bot_row().await;
        harness
            .mount("POST", "/revoke", ResponseTemplate::new(200))
            .await;
        let broadcaster_before = harness.repo.raw(TWITCH_CREDENTIAL_ID);

        let result = harness
            .bot
            .adopt_grant(grant_for(SELF_USER_ID, "streamer"))
            .await;

        assert!(
            matches!(result, Err(BotLoginError::SameAsBroadcaster)),
            "{result:?}"
        );
        assert!(harness.repo.raw(TWITCH_BOT_CREDENTIAL_ID).is_none());
        assert_eq!(harness.repo.raw(TWITCH_CREDENTIAL_ID), broadcaster_before);
        assert_eq!(harness.bot.status(), BotStatus::NotConnected);
        let revoked = revoked_tokens(&harness).await;
        assert_eq!(revoked.len(), 2, "{revoked:?}");
        assert!(
            revoked
                .iter()
                .any(|body| body.contains("GRANT_ACCESS_SENTINEL_p3"))
        );
        assert!(
            revoked
                .iter()
                .any(|body| body.contains("GRANT_REFRESH_SENTINEL_p3"))
        );
    }

    #[tokio::test]
    async fn adopting_a_grant_for_another_account_stores_the_bot_row_and_connects() {
        let harness = BotHarness::without_bot_row().await;

        let status = harness
            .bot
            .adopt_grant(grant_for(BOT_USER_ID, BOT_LOGIN))
            .await
            .unwrap();

        let expected = BotStatus::Connected {
            login: BOT_LOGIN.to_owned(),
        };
        assert_eq!(status, expected);
        assert_eq!(harness.bot.status(), expected);
        assert_eq!(
            harness.bot.connected_user_id().as_deref(),
            Some(BOT_USER_ID)
        );
        let stored = crate::credentials::load_at(harness.repo.as_ref(), TWITCH_BOT_CREDENTIAL_ID)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.user_id, BOT_USER_ID);
        assert_eq!(stored.access_token.expose(), "GRANT_ACCESS_SENTINEL_p3");
        assert!(harness.requests_to("/revoke").await.is_empty());
        assert_eq!(
            crate::credentials::load(harness.repo.as_ref())
                .await
                .unwrap()
                .unwrap()
                .user_id,
            SELF_USER_ID,
            "the streamer row must stay the streamer's"
        );
    }
}
