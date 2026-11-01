mod login;
mod moderator;
mod validate;

use forge_types::RecoverRwLock;
use std::sync::{Arc, RwLock};

use forge_events::{EventPublisher, EventSource, TypedEvent};
use forge_net::Gateway;
use forge_storage::CredentialsRepo;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::auth::TwitchAuthFlow;
use crate::credentials::{TWITCH_BOT_CREDENTIAL_ID, load_at};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport};
use crate::oauth_client::TwitchOAuthClient;
use crate::vocabulary::BotStatusChanged;
use validate::{TokenValidator, TokenVerdict};

pub use moderator::BotModerator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotStatus {
    NotConnected,
    Connected { login: String },
    ReauthRequired { login: String },
}

impl BotStatus {
    pub fn login(&self) -> Option<&str> {
        match self {
            Self::NotConnected => None,
            Self::Connected { login } | Self::ReauthRequired { login } => Some(login),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotUnavailable {
    NotConnected,
    ReauthRequired,
    StorageFailure,
}

impl std::fmt::Display for BotUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotConnected => "no bot account signed in",
            Self::ReauthRequired => "bot account needs to sign in again",
            Self::StorageFailure => "bot credentials could not be read",
        })
    }
}

pub(crate) struct BotIdentity {
    pub(crate) user_id: String,
    pub(crate) login: String,
    pub(crate) transport: Arc<dyn HelixTransport>,
}

struct BotState {
    status: BotStatus,
    user_id: Option<String>,
    token_rejected: bool,
    moderator: BotModerator,
}

pub struct TwitchBotAccount {
    repo: Arc<dyn CredentialsRepo>,
    manager: Arc<TwitchCredentialsManager>,
    transport: Arc<dyn HelixTransport>,
    validator: TokenValidator,
    oauth: TwitchOAuthClient,
    helix: Gateway,
    bus: Arc<dyn EventPublisher>,
    state: RwLock<BotState>,
    broadcaster_transport: Arc<dyn HelixTransport>,
    pending_flow: tokio::sync::Mutex<Option<TwitchAuthFlow>>,
    login_cancel: std::sync::Mutex<CancellationToken>,
}

impl TwitchBotAccount {
    pub fn new(
        repo: Arc<dyn CredentialsRepo>,
        oauth: TwitchOAuthClient,
        bus: Arc<dyn EventPublisher>,
        helix: Gateway,
        broadcaster_transport: Arc<dyn HelixTransport>,
    ) -> Arc<Self> {
        let client_id = oauth.client_id().to_owned();
        let manager = Arc::new(TwitchCredentialsManager::for_bot(
            oauth.clone(),
            Arc::clone(&repo),
        ));
        let transport: Arc<dyn HelixTransport> = Arc::new(
            HelixHttpTransport::new(
                helix.clone(),
                client_id.clone(),
                Arc::clone(&manager) as Arc<dyn HelixTokenSource>,
            )
            .with_refresher(Arc::clone(&manager) as Arc<dyn HelixTokenRefresher>),
        );
        Arc::new(Self {
            repo,
            manager,
            transport,
            validator: TokenValidator::new(oauth.clone()),
            oauth,
            helix,
            bus,
            state: RwLock::new(BotState {
                status: BotStatus::NotConnected,
                user_id: None,
                token_rejected: false,
                moderator: BotModerator::Unknown,
            }),
            broadcaster_transport,
            pending_flow: tokio::sync::Mutex::new(None),
            login_cancel: std::sync::Mutex::new(CancellationToken::new()),
        })
    }

    pub fn moderator_status(&self) -> BotModerator {
        self.state.read_recover().moderator
    }

    pub fn status(&self) -> BotStatus {
        self.state.read_recover().status.clone()
    }

    pub(crate) fn connected_user_id(&self) -> Option<String> {
        self.state.read_recover().user_id.clone()
    }

    pub async fn sync_status(&self) {
        let stored = match load_at(self.repo.as_ref(), TWITCH_BOT_CREDENTIAL_ID).await {
            Ok(stored) => stored,
            Err(e) => {
                warn!(error = %e, "bot credential unreadable");
                return;
            }
        };
        let Some(stored) = stored else {
            self.set_state(BotStatus::NotConnected, None);
            return;
        };
        let usable = match self.manager.get_valid_access_token().await {
            Ok(token) => self.validator.check(&token).await != TokenVerdict::Invalid,
            Err(forge_platform_core::PlatformError::ReauthRequired { .. }) => false,
            Err(_) => true,
        };
        let status = if usable {
            BotStatus::Connected {
                login: stored.login,
            }
        } else {
            BotStatus::ReauthRequired {
                login: stored.login,
            }
        };
        let connected = matches!(status, BotStatus::Connected { .. });
        self.set_state(status, Some(stored.user_id));
        if connected {
            self.refresh_moderator_status().await;
        }
    }

    pub(crate) async fn identity(&self) -> Result<BotIdentity, BotUnavailable> {
        let stored = match load_at(self.repo.as_ref(), TWITCH_BOT_CREDENTIAL_ID).await {
            Ok(Some(stored)) => stored,
            Ok(None) => return Err(BotUnavailable::NotConnected),
            Err(e) => {
                warn!(error = %e, "bot credential unreadable");
                return Err(BotUnavailable::StorageFailure);
            }
        };
        if self.token_known_rejected() {
            return Err(BotUnavailable::ReauthRequired);
        }
        Ok(BotIdentity {
            user_id: stored.user_id,
            login: stored.login,
            transport: Arc::clone(&self.transport),
        })
    }

    fn token_known_rejected(&self) -> bool {
        let guard = self.state.read_recover();
        guard.token_rejected || matches!(guard.status, BotStatus::ReauthRequired { .. })
    }

    pub(crate) fn mark_reauth_required(&self) {
        let changed = {
            let mut guard = self.state.write_recover();
            guard.token_rejected = true;
            let Some(login) = guard.status.login().map(str::to_owned) else {
                return;
            };
            let status = BotStatus::ReauthRequired { login };
            let changed = guard.status != status;
            guard.status = status;
            changed
        };
        if changed {
            self.publish_status();
        }
    }

    pub(crate) async fn token_confirmed_invalid(&self) -> bool {
        match self.manager.get_valid_access_token().await {
            Ok(token) => self.validator.check(&token).await == TokenVerdict::Invalid,
            Err(forge_platform_core::PlatformError::ReauthRequired { .. }) => true,
            Err(_) => false,
        }
    }

    fn set_state(&self, status: BotStatus, user_id: Option<String>) {
        let changed = {
            let mut guard = self.state.write_recover();
            let changed = guard.status != status;
            if status == BotStatus::NotConnected {
                guard.moderator = BotModerator::Unknown;
            }
            guard.status = status;
            guard.user_id = user_id;
            guard.token_rejected = false;
            changed
        };
        if changed {
            self.publish_status();
        }
    }

    fn set_moderator(&self, moderator: BotModerator) {
        let changed = {
            let mut guard = self.state.write_recover();
            let changed = guard.moderator != moderator;
            guard.moderator = moderator;
            changed
        };
        if changed {
            self.publish_status();
        }
    }

    fn publish_status(&self) {
        let (status, moderator) = {
            let guard = self.state.read_recover();
            (guard.status.clone(), guard.moderator)
        };
        let changed = BotStatusChanged::new(&status, moderator);
        info!(
            status = ?changed.status,
            moderator = ?changed.moderator,
            "twitch bot account status changed"
        );
        changed.publish(self.bus.as_ref(), EventSource::Twitch, None);
    }
}

#[cfg(test)]
mod tests {
    use wiremock::ResponseTemplate;

    use super::*;
    use crate::auth::TWITCH_BOT_SCOPES;
    use crate::test_support::bot_account::{BOT_LOGIN, BotHarness, CLIENT_ID};

    fn connected() -> BotStatus {
        BotStatus::Connected {
            login: BOT_LOGIN.to_owned(),
        }
    }

    fn reauth() -> BotStatus {
        BotStatus::ReauthRequired {
            login: BOT_LOGIN.to_owned(),
        }
    }

    #[tokio::test]
    async fn sync_status_maps_each_validation_verdict_to_a_bot_status() {
        let cases = [
            ("valid", 200, connected()),
            ("invalid", 401, reauth()),
            ("inconclusive", 503, connected()),
        ];

        for (label, status, expected) in cases {
            let harness = BotHarness::with_bot_row().await;
            harness
                .mount_validate(status, TWITCH_BOT_SCOPES, CLIENT_ID)
                .await;

            harness.bot.sync_status().await;

            assert_eq!(harness.bot.status(), expected, "{label}");
        }
    }

    #[tokio::test]
    async fn sync_status_marks_reauth_when_the_expired_bot_token_cannot_be_refreshed() {
        let harness = BotHarness::without_bot_row().await;
        harness.repo.put(
            TWITCH_BOT_CREDENTIAL_ID,
            serde_json::json!({
                "access_token": "expired_bot_access",
                "refresh_token": "revoked_bot_refresh",
                "user_id": crate::test_support::bot_account::BOT_USER_ID,
                "login": BOT_LOGIN,
                "expires_at_unix": 1,
            })
            .to_string(),
        );
        harness
            .mount(
                "POST",
                "/token",
                ResponseTemplate::new(reqwest::StatusCode::BAD_REQUEST).set_body_json(
                    serde_json::json!({
                        "status": 400,
                        "message": "Invalid refresh token",
                    }),
                ),
            )
            .await;

        harness.bot.sync_status().await;

        assert_eq!(harness.bot.status(), reauth());
        assert!(
            harness.requests_to("/validate").await.is_empty(),
            "a token that cannot be refreshed must not be validated"
        );
    }

    #[tokio::test]
    async fn sync_status_without_a_bot_row_stays_not_connected_and_publishes_nothing() {
        let harness = BotHarness::without_bot_row().await;

        harness.bot.sync_status().await;

        assert_eq!(harness.bot.status(), BotStatus::NotConnected);
        assert!(harness.bot.connected_user_id().is_none());
        assert!(
            harness
                .bus
                .of_kind(crate::vocabulary::event_kinds::BOT_STATUS_CHANGED)
                .is_empty()
        );
    }

    #[tokio::test]
    async fn status_event_is_published_once_per_change_with_the_wire_status_and_login() {
        let harness = BotHarness::connected().await;
        harness
            .mount_validate(200, TWITCH_BOT_SCOPES, CLIENT_ID)
            .await;
        harness.bot.sync_status().await;

        harness.bot.mark_reauth_required();

        let payloads: Vec<serde_json::Value> = harness
            .bus
            .of_kind(crate::vocabulary::event_kinds::BOT_STATUS_CHANGED)
            .into_iter()
            .map(|event| event.payload)
            .collect();
        assert_eq!(
            payloads,
            vec![
                serde_json::json!({"status": "connected", "login": BOT_LOGIN, "moderator": "unknown"}),
                serde_json::json!({"status": "reauth_required", "login": BOT_LOGIN, "moderator": "unknown"}),
            ]
        );
    }

    #[tokio::test]
    async fn mark_reauth_required_makes_the_bot_identity_unavailable() {
        let harness = BotHarness::connected().await;
        assert!(harness.bot.identity().await.is_ok());

        harness.bot.mark_reauth_required();

        assert!(matches!(
            harness.bot.identity().await,
            Err(BotUnavailable::ReauthRequired)
        ));
    }
}
