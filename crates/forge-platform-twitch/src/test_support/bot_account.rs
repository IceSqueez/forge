use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use time::OffsetDateTime;
use tokio::sync::Notify;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::auth::TWITCH_BOT_SCOPES;
use crate::bot_account::TwitchBotAccount;
use crate::credentials::{TWITCH_BOT_CREDENTIAL_ID, TWITCH_CREDENTIAL_ID};
use crate::helix::HelixTransport;
use crate::test_support::{SELF_USER_ID, TOKEN_SENTINEL, endpoints_with};

pub(crate) const CLIENT_ID: &str = "test-client";
pub(crate) const BOT_USER_ID: &str = "4242";
pub(crate) const BOT_LOGIN: &str = "helper_bot";
pub(crate) const BOT_ACCESS: &str = "BOT_ACCESS_SENTINEL_z9k";
pub(crate) const BOT_REFRESH: &str = "BOT_REFRESH_SENTINEL_z9k";

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

#[derive(Default)]
pub(crate) struct KeyedRepo(
    Mutex<HashMap<String, String>>,
    Mutex<Option<fn() -> StorageError>>,
    Mutex<Option<fn() -> StorageError>>,
);

impl KeyedRepo {
    pub(crate) fn raw(&self, id: &str) -> Option<String> {
        self.0.lock().unwrap().get(id).cloned()
    }

    pub(crate) fn put(&self, id: &str, json: String) {
        self.0.lock().unwrap().insert(id.to_owned(), json);
    }

    pub(crate) fn fail_loads_with(&self, error: fn() -> StorageError) {
        *self.1.lock().unwrap() = Some(error);
    }

    pub(crate) fn fail_stores_with(&self, error: fn() -> StorageError) {
        *self.2.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl CredentialsRepo for KeyedRepo {
    async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
        if let Some(error) = *self.2.lock().unwrap() {
            return Err(error());
        }
        self.put(id.as_str(), plaintext.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        if let Some(error) = *self.1.lock().unwrap() {
            return Err(error());
        }
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

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

pub(crate) fn credential_json(access: &str, refresh: &str, user_id: &str, login: &str) -> String {
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
pub(crate) struct RecordingBus {
    pub(crate) events: Mutex<Vec<Event>>,
    pub(crate) published: tokio::sync::Notify,
}

impl EventPublisher for RecordingBus {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
        self.published.notify_one();
    }
}

impl RecordingBus {
    pub(crate) fn of_kind(&self, kind: &str) -> Vec<Event> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }

    pub(crate) fn taken(&self) -> Vec<Event> {
        std::mem::take(&mut *self.events.lock().unwrap())
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
    pub(crate) broadcaster: Arc<RoutedTransport>,
}

pub(crate) struct RoutedTransport(Mutex<Arc<dyn HelixTransport>>);

impl RoutedTransport {
    fn unreachable() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Arc::new(UnreachableTransport))))
    }

    pub(crate) fn route_to(&self, transport: Arc<dyn HelixTransport>) {
        *self.0.lock().unwrap() = transport;
    }
}

#[async_trait]
impl HelixTransport for RoutedTransport {
    async fn execute(
        &self,
        request: crate::helix::HelixRequest,
    ) -> Result<serde_json::Value, crate::error::TwitchError> {
        let transport = Arc::clone(&self.0.lock().unwrap());
        transport.execute(request).await
    }
}

struct UnreachableTransport;

#[async_trait]
impl HelixTransport for UnreachableTransport {
    async fn execute(
        &self,
        _: crate::helix::HelixRequest,
    ) -> Result<serde_json::Value, crate::error::TwitchError> {
        Err(crate::error::TwitchError::Transport("offline".to_owned()))
    }
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
            (crate::endpoint_surfaces::TWITCH_API, server.uri().as_str()),
            (
                crate::endpoint_surfaces::TWITCH_OAUTH,
                server.uri().as_str(),
            ),
        ]);
        let broadcaster = RoutedTransport::unreachable();
        let bot = TwitchBotAccount::new(
            bot_repo,
            crate::test_support::oauth_client_at(&server.uri(), CLIENT_ID),
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
            Arc::clone(&broadcaster) as Arc<dyn HelixTransport>,
        );
        Self {
            server,
            repo,
            bus,
            bot,
            broadcaster,
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
