mod login;
mod validate;

use std::sync::{Arc, RwLock};

use forge_events::{Event, EventPublisher, EventSource};
use forge_platform_core::{PlatformEndpoints, TokenBucketRateLimiter};
use forge_storage::CredentialsRepo;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::auth::TwitchAuthFlow;
use crate::builtin::{HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW};
use crate::credentials::{TWITCH_BOT_CREDENTIAL_ID, load_at};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport};
use validate::{TokenValidator, TokenVerdict};

pub use login::BotLoginError;

pub const BOT_STATUS_CHANGED_KIND: &str = "twitch.bot.status";

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

    fn wire_name(&self) -> &'static str {
        match self {
            Self::NotConnected => "not_connected",
            Self::Connected { .. } => "connected",
            Self::ReauthRequired { .. } => "reauth_required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BotUnavailable {
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
}

pub struct TwitchBotAccount {
    repo: Arc<dyn CredentialsRepo>,
    manager: Arc<TwitchCredentialsManager>,
    transport: Arc<dyn HelixTransport>,
    validator: TokenValidator,
    endpoints: PlatformEndpoints,
    client_id: String,
    bus: Arc<dyn EventPublisher>,
    state: RwLock<BotState>,
    pending_flow: tokio::sync::Mutex<Option<TwitchAuthFlow>>,
    login_cancel: std::sync::Mutex<CancellationToken>,
}

impl TwitchBotAccount {
    pub fn new(
        endpoints: &PlatformEndpoints,
        repo: Arc<dyn CredentialsRepo>,
        client_id: String,
        bus: Arc<dyn EventPublisher>,
    ) -> Arc<Self> {
        let manager = Arc::new(TwitchCredentialsManager::for_bot(
            endpoints,
            Arc::clone(&repo),
            client_id.clone(),
        ));
        let transport: Arc<dyn HelixTransport> = Arc::new(
            HelixHttpTransport::new(
                endpoints,
                Arc::new(TokenBucketRateLimiter::new(
                    HELIX_BUDGET_CAPACITY,
                    HELIX_BUDGET_WINDOW,
                )),
                Arc::clone(&bus),
                client_id.clone(),
                Arc::clone(&manager) as Arc<dyn HelixTokenSource>,
            )
            .with_refresher(Arc::clone(&manager) as Arc<dyn HelixTokenRefresher>),
        );
        Arc::new(Self {
            repo,
            manager,
            transport,
            validator: TokenValidator::new(endpoints, client_id.clone()),
            endpoints: endpoints.clone(),
            client_id,
            bus,
            state: RwLock::new(BotState {
                status: BotStatus::NotConnected,
                user_id: None,
                token_rejected: false,
            }),
            pending_flow: tokio::sync::Mutex::new(None),
            login_cancel: std::sync::Mutex::new(CancellationToken::new()),
        })
    }

    pub fn status(&self) -> BotStatus {
        self.state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .status
            .clone()
    }

    pub(crate) fn connected_user_id(&self) -> Option<String> {
        self.state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .user_id
            .clone()
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
        self.set_state(status, Some(stored.user_id));
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
        let guard = self.state.read().unwrap_or_else(|p| p.into_inner());
        guard.token_rejected || matches!(guard.status, BotStatus::ReauthRequired { .. })
    }

    pub(crate) fn mark_reauth_required(&self) {
        let (login, user_id) = {
            let mut guard = self.state.write().unwrap_or_else(|p| p.into_inner());
            guard.token_rejected = true;
            (
                guard.status.login().map(str::to_owned),
                guard.user_id.clone(),
            )
        };
        if let Some(login) = login {
            self.set_state(BotStatus::ReauthRequired { login }, user_id);
            let mut guard = self.state.write().unwrap_or_else(|p| p.into_inner());
            guard.token_rejected = true;
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
            let mut guard = self.state.write().unwrap_or_else(|p| p.into_inner());
            let changed = guard.status != status;
            guard.status = status.clone();
            guard.user_id = user_id;
            guard.token_rejected = false;
            changed
        };
        if !changed {
            return;
        }
        info!(
            status = status.wire_name(),
            "twitch bot account status changed"
        );
        self.bus.publish(Event::new(
            EventSource::Twitch,
            BOT_STATUS_CHANGED_KIND,
            serde_json::json!({
                "status": status.wire_name(),
                "login": status.login(),
            }),
        ));
    }
}
