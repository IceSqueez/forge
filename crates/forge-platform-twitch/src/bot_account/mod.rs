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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod test_support {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::EndpointSurface;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::OffsetDateTime;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::TwitchBotAccount;
    use crate::auth::TWITCH_BOT_SCOPES;
    use crate::credentials::{TWITCH_BOT_CREDENTIAL_ID, TWITCH_CREDENTIAL_ID};
    use crate::sub_actions::test_support::{SELF_USER_ID, TOKEN_SENTINEL, endpoints_with};

    pub(crate) const CLIENT_ID: &str = "test-client";
    pub(crate) const BOT_USER_ID: &str = "4242";
    pub(crate) const BOT_LOGIN: &str = "helper_bot";
    pub(crate) const BOT_ACCESS: &str = "BOT_ACCESS_SENTINEL_z9k";
    pub(crate) const BOT_REFRESH: &str = "BOT_REFRESH_SENTINEL_z9k";

    #[derive(Default)]
    pub(crate) struct KeyedRepo(Mutex<HashMap<String, String>>);

    impl KeyedRepo {
        pub(crate) fn raw(&self, id: &str) -> Option<String> {
            self.0.lock().unwrap().get(id).cloned()
        }

        pub(crate) fn put(&self, id: &str, json: String) {
            self.0.lock().unwrap().insert(id.to_owned(), json);
        }
    }

    #[async_trait]
    impl CredentialsRepo for KeyedRepo {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.put(id.as_str(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.raw(id.as_str()))
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .keys()
                .map(|id| CredentialId::new(id.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    pub(crate) fn credential_json(
        access: &str,
        refresh: &str,
        user_id: &str,
        login: &str,
    ) -> String {
        serde_json::json!({
            "access_token": access,
            "refresh_token": refresh,
            "user_id": user_id,
            "login": login,
            "expires_at_unix": null,
        })
        .to_string()
    }

    #[derive(Default)]
    pub(crate) struct RecordingBus(Mutex<Vec<Event>>);

    impl EventPublisher for RecordingBus {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl RecordingBus {
        pub(crate) fn of_kind(&self, kind: &str) -> Vec<Event> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.kind == kind)
                .cloned()
                .collect()
        }
    }

    fn validate_response(status: u16, scopes: &[&str], client_id: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_json(serde_json::json!({
            "client_id": client_id,
            "login": BOT_LOGIN,
            "user_id": BOT_USER_ID,
            "scopes": scopes,
            "expires_in": 3600,
        }))
    }

    pub(crate) struct BotHarness {
        pub(crate) server: MockServer,
        pub(crate) repo: Arc<KeyedRepo>,
        pub(crate) bus: Arc<RecordingBus>,
        pub(crate) bot: Arc<TwitchBotAccount>,
    }

    impl BotHarness {
        pub(crate) async fn with_bot_row() -> Self {
            let harness = Self::without_bot_row().await;
            harness.repo.put(
                TWITCH_BOT_CREDENTIAL_ID,
                credential_json(BOT_ACCESS, BOT_REFRESH, BOT_USER_ID, BOT_LOGIN),
            );
            harness
        }

        pub(crate) async fn without_bot_row() -> Self {
            let server = MockServer::start().await;
            let repo = Arc::new(KeyedRepo::default());
            repo.put(
                TWITCH_CREDENTIAL_ID,
                credential_json(TOKEN_SENTINEL, "streamer_refresh", SELF_USER_ID, "streamer"),
            );
            let bus = Arc::new(RecordingBus::default());
            let endpoints = endpoints_with(&[
                (EndpointSurface::TwitchApi, server.uri().as_str()),
                (EndpointSurface::TwitchOAuth, server.uri().as_str()),
            ]);
            let bot = TwitchBotAccount::new(
                &endpoints,
                Arc::clone(&repo) as Arc<dyn CredentialsRepo>,
                CLIENT_ID.to_owned(),
                Arc::clone(&bus) as Arc<dyn EventPublisher>,
            );
            Self {
                server,
                repo,
                bus,
                bot,
            }
        }

        pub(crate) async fn connected() -> Self {
            let harness = Self::with_bot_row().await;
            Mock::given(method("GET"))
                .and(path("/validate"))
                .respond_with(validate_response(200, TWITCH_BOT_SCOPES, CLIENT_ID))
                .up_to_n_times(1)
                .mount(&harness.server)
                .await;
            harness.bot.sync_status().await;
            harness
        }

        pub(crate) async fn mount_validate(&self, status: u16, scopes: &[&str], client_id: &str) {
            Mock::given(method("GET"))
                .and(path("/validate"))
                .respond_with(validate_response(status, scopes, client_id))
                .mount(&self.server)
                .await;
        }

        pub(crate) async fn mount(&self, verb: &str, route: &str, response: ResponseTemplate) {
            Mock::given(method(verb))
                .and(path(route))
                .respond_with(response)
                .mount(&self.server)
                .await;
        }

        pub(crate) async fn requests_to(&self, route: &str) -> Vec<wiremock::Request> {
            self.server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .filter(|request| request.url.path() == route)
                .collect()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use wiremock::ResponseTemplate;

    use super::test_support::{BOT_LOGIN, BotHarness, CLIENT_ID};
    use super::*;
    use crate::auth::TWITCH_BOT_SCOPES;

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
    async fn sync_status_classifies_the_stored_bot_token_by_its_validation_answer() {
        let missing_whispers: Vec<&str> = TWITCH_BOT_SCOPES
            .iter()
            .copied()
            .filter(|scope| *scope != "user:manage:whispers")
            .collect();
        let cases: [(&str, u16, &[&str], &str, BotStatus); 5] = [
            ("valid", 200, TWITCH_BOT_SCOPES, CLIENT_ID, connected()),
            ("rejected", 401, TWITCH_BOT_SCOPES, CLIENT_ID, reauth()),
            ("missing scope", 200, &missing_whispers, CLIENT_ID, reauth()),
            (
                "foreign client",
                200,
                TWITCH_BOT_SCOPES,
                "other-app",
                reauth(),
            ),
            (
                "validator down",
                503,
                TWITCH_BOT_SCOPES,
                CLIENT_ID,
                connected(),
            ),
        ];

        for (label, status, scopes, client_id, expected) in cases {
            let harness = BotHarness::with_bot_row().await;
            harness.mount_validate(status, scopes, client_id).await;

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
                "user_id": super::test_support::BOT_USER_ID,
                "login": BOT_LOGIN,
                "expires_at_unix": 1,
            })
            .to_string(),
        );
        harness
            .mount(
                "POST",
                "/token",
                ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "status": 400,
                    "message": "Invalid refresh token",
                })),
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
        assert!(harness.bus.of_kind(BOT_STATUS_CHANGED_KIND).is_empty());
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
            .of_kind(BOT_STATUS_CHANGED_KIND)
            .into_iter()
            .map(|event| event.payload)
            .collect();
        assert_eq!(
            payloads,
            vec![
                serde_json::json!({"status": "connected", "login": BOT_LOGIN}),
                serde_json::json!({"status": "reauth_required", "login": BOT_LOGIN}),
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
