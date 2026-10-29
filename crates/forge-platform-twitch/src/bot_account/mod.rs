mod login;
mod moderator;
mod validate;

use std::sync::{Arc, OnceLock, RwLock};

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

pub use login::BotLoginError;
pub use moderator::{BotModerator, BotModeratorError};

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
    broadcaster_transport: OnceLock<Arc<dyn HelixTransport>>,
    pending_flow: tokio::sync::Mutex<Option<TwitchAuthFlow>>,
    login_cancel: std::sync::Mutex<CancellationToken>,
}

impl TwitchBotAccount {
    pub fn new(
        repo: Arc<dyn CredentialsRepo>,
        oauth: TwitchOAuthClient,
        bus: Arc<dyn EventPublisher>,
        helix: Gateway,
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
            broadcaster_transport: OnceLock::new(),
            pending_flow: tokio::sync::Mutex::new(None),
            login_cancel: std::sync::Mutex::new(CancellationToken::new()),
        })
    }

    pub fn attach_broadcaster_transport(&self, transport: Arc<dyn HelixTransport>) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the first attached transport wins"
        )]
        let _ = self.broadcaster_transport.set(transport);
    }

    pub fn moderator_status(&self) -> BotModerator {
        self.state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .moderator
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
            let mut guard = self.state.write().unwrap_or_else(|p| p.into_inner());
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
            let guard = self.state.read().unwrap_or_else(|p| p.into_inner());
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
pub(crate) mod test_support {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::EndpointSurface;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::OffsetDateTime;
    use tokio::sync::Notify;
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
    const SETTLE_TURNS: usize = 32;

    #[derive(Default)]
    pub(crate) struct StoreGate {
        armed: AtomicBool,
        arrived: Notify,
        release: Notify,
    }

    impl StoreGate {
        pub(crate) fn armed() -> Arc<Self> {
            let gate = Self::default();
            gate.armed.store(true, Ordering::SeqCst);
            Arc::new(gate)
        }

        pub(crate) async fn arrival(&self) {
            self.arrived.notified().await;
        }

        pub(crate) fn open(&self) {
            self.release.notify_one();
        }

        async fn pass(&self) {
            if self.armed.swap(false, Ordering::SeqCst) {
                self.arrived.notify_one();
                self.release.notified().await;
            }
        }
    }

    pub(crate) struct GatedRepo {
        rows: Arc<KeyedRepo>,
        gate: Arc<StoreGate>,
    }

    impl GatedRepo {
        pub(crate) fn over(rows: Arc<KeyedRepo>, gate: Arc<StoreGate>) -> Arc<Self> {
            Arc::new(Self { rows, gate })
        }
    }

    #[async_trait]
    impl CredentialsRepo for GatedRepo {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.gate.pass().await;
            self.rows.store(id, plaintext).await
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            self.rows.load(id).await
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            self.rows.delete(id).await
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            self.rows.list_ids().await
        }

        async fn last_refresh(
            &self,
            id: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            self.rows.last_refresh(id).await
        }

        async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError> {
            self.rows.mark_refreshed(id).await
        }
    }

    pub(crate) async fn settle() {
        for _ in 0..SETTLE_TURNS {
            tokio::task::yield_now().await;
        }
    }

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

        pub(crate) async fn with_bot_row_gated_by(gate: Arc<StoreGate>) -> Self {
            let harness = Self::over(Some(gate)).await;
            harness.repo.put(
                TWITCH_BOT_CREDENTIAL_ID,
                credential_json(BOT_ACCESS, BOT_REFRESH, BOT_USER_ID, BOT_LOGIN),
            );
            harness
        }

        pub(crate) async fn without_bot_row() -> Self {
            Self::over(None).await
        }

        async fn over(gate: Option<Arc<StoreGate>>) -> Self {
            let server = MockServer::start().await;
            let repo = Arc::new(KeyedRepo::default());
            repo.put(
                TWITCH_CREDENTIAL_ID,
                credential_json(TOKEN_SENTINEL, "streamer_refresh", SELF_USER_ID, "streamer"),
            );
            let bot_repo: Arc<dyn CredentialsRepo> = match gate {
                Some(gate) => GatedRepo::over(Arc::clone(&repo), gate),
                None => Arc::clone(&repo) as Arc<dyn CredentialsRepo>,
            };
            let bus = Arc::new(RecordingBus::default());
            let endpoints = endpoints_with(&[
                (EndpointSurface::TwitchApi, server.uri().as_str()),
                (EndpointSurface::TwitchOAuth, server.uri().as_str()),
            ]);
            let bot = TwitchBotAccount::new(
                bot_repo,
                crate::sub_actions::test_support::oauth_client_at(&server.uri(), CLIENT_ID),
                Arc::clone(&bus) as Arc<dyn EventPublisher>,
                crate::helix_gateway::helix_gateway(
                    &endpoints,
                    Arc::new(forge_net::TokenBucketRateLimiter::new(
                        crate::config::HELIX_BUDGET_CAPACITY,
                        crate::config::HELIX_BUDGET_WINDOW,
                    )),
                    Arc::clone(&bus) as Arc<dyn EventPublisher>,
                )
                .unwrap(),
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
