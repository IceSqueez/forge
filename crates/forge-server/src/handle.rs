use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_runtime::{OverlayConnectListener, OverlayReceivers, OverlayServiceHandle};
use forge_storage::OverlayId;
use forge_storage::paths;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, watch};

use crate::audio_clips::{ClipCapability, ClipOffer, ClipOutcomeHandle, ClipTicket};
use crate::auth::AuthState;
use crate::config::ServerSettings;
use crate::origin::build_allowed_origins;
use crate::run_state::{Generation, RunState};
use crate::server::AppState;
use crate::{SecretToken, ServerError, server};

struct HandleInner {
    join: Option<tokio::task::JoinHandle<Result<(), ServerError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    state: AppState,
    bind_addr: SocketAddr,
}

#[derive(Clone)]
pub struct ServerHandle {
    inner: Arc<Mutex<HandleInner>>,
    run_state: RunState,
}

impl ServerHandle {
    pub(crate) fn new(
        join: tokio::task::JoinHandle<Result<(), ServerError>>,
        shutdown_tx: watch::Sender<bool>,
        state: AppState,
        bind_addr: SocketAddr,
        run_state_tx: watch::Sender<bool>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HandleInner {
                join: Some(join),
                shutdown_tx: Some(shutdown_tx),
                state,
                bind_addr,
            })),
            run_state: RunState::new(run_state_tx),
        }
    }

    pub(crate) fn stopped(state: AppState, bind_addr: SocketAddr) -> Self {
        let (run_state_tx, _run_state_rx) = watch::channel(false);
        Self {
            inner: Arc::new(Mutex::new(HandleInner {
                join: None,
                shutdown_tx: None,
                state,
                bind_addr,
            })),
            run_state: RunState::new(run_state_tx),
        }
    }

    pub(crate) fn adopt_generation(&self, mut reported: watch::Receiver<bool>) -> Generation {
        let generation = self.run_state.claim();
        let run_state = self.run_state.clone();
        tokio::spawn(async move {
            while reported.changed().await.is_ok() {
                if !*reported.borrow_and_update() {
                    break;
                }
            }
            run_state.report_stopped(generation);
        });
        generation
    }

    pub fn run_state(&self) -> watch::Receiver<bool> {
        self.run_state.subscribe()
    }

    pub async fn stop(&self) -> Result<(), ServerError> {
        let (shutdown_tx, join, bus_adapter) = {
            let mut guard = self.inner.lock().await;

            if guard.shutdown_tx.is_none() {
                return Ok(());
            }

            guard.state.audio_clips.discard_all();

            (
                guard.shutdown_tx.take(),
                guard.join.take(),
                Arc::clone(&guard.state.bus_adapter),
            )
        };

        if let Some(tx) = shutdown_tx {
            let _ = tx.send(true);
        }

        self.run_state.stop();

        bus_adapter.broadcast_close().await;

        let drain = async {
            if let Some(join) = join {
                let _ = join.await;
            }
        };

        let _ = tokio::time::timeout(Duration::from_secs(5), drain).await;

        Ok(())
    }

    pub async fn restart(&self) -> Result<(), ServerError> {
        let state = {
            let guard = self.inner.lock().await;
            guard.state.clone()
        };

        let settings = ServerSettings::load(state.settings.as_ref())
            .await
            .map_err(|e| ServerError::Storage(e.to_string()))?;

        if !settings.enabled {
            self.stop().await?;
            return Err(ServerError::Disabled);
        }

        let ip: IpAddr = settings
            .bind_address
            .parse()
            .map_err(|_| ServerError::Bind {
                addr: settings.bind_address.clone(),
                reason: "invalid bind address".to_owned(),
            })?;
        let bind_addr = SocketAddr::new(ip, settings.port);
        let overlay_root = settings
            .overlay_root
            .filter(|root| !root.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(paths::overlays_dir);

        server::validate_lan_bind(
            &bind_addr,
            settings.lan_bind_enabled,
            state.credentials.as_ref(),
        )
        .await?;

        self.stop().await?;
        state
            .auth
            .set_reads_required(settings.auth_required_for_reads);
        state
            .auth
            .overlay_token()
            .set_required(settings.http_overlay_require_token);

        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|e| ServerError::Bind {
                addr: bind_addr.to_string(),
                reason: e.to_string(),
            })?;
        let bind_addr = listener.local_addr().unwrap_or(bind_addr);
        let allowed_origins = Arc::new(build_allowed_origins(
            bind_addr,
            &settings.additional_origins,
        ));

        let new_state = AppState {
            auth: Arc::clone(&state.auth),
            bus: Arc::clone(&state.bus),
            bus_adapter: Arc::clone(&state.bus_adapter),
            actions: Arc::clone(&state.actions),
            globals: Arc::clone(&state.globals),
            user_globals: Arc::clone(&state.user_globals),
            overlays: Arc::clone(&state.overlays),
            credentials: Arc::clone(&state.credentials),
            settings: Arc::clone(&state.settings),
            server_info: Arc::clone(&state.server_info),
            action_engine: Arc::clone(&state.action_engine),
            audio_clips: Arc::clone(&state.audio_clips),
            overlay_root: Arc::new(overlay_root),
            overlay_cors_any_origin: settings.overlay_cors_any_origin,
            bind_addr,
            allowed_origins,
        };

        let (reported_tx, reported_rx) = watch::channel(true);
        let (join, shutdown_tx) =
            server::serve_on_with_shutdown(listener, new_state.clone(), reported_tx);
        let generation = self.adopt_generation(reported_rx);

        let mut guard = self.inner.lock().await;
        guard.join = Some(join);
        guard.shutdown_tx = Some(shutdown_tx);
        guard.state = new_state;
        guard.bind_addr = bind_addr;
        drop(guard);

        self.run_state.report_running(generation);

        Ok(())
    }

    pub async fn auth_state(&self) -> Arc<AuthState> {
        Arc::clone(&self.inner.lock().await.state.auth)
    }

    pub async fn set_http_overlay_token_required(&self, required: bool) {
        self.auth_state()
            .await
            .overlay_token()
            .set_required(required);
    }

    pub async fn regenerate_overlay_token(
        &self,
        overlay_service: &OverlayServiceHandle,
    ) -> Result<String, ServerError> {
        let state = self.inner.lock().await.state.clone();
        let gate = state.auth.overlay_token();
        let fresh = gate.stage_rotation(state.credentials.as_ref()).await?;
        let reissued = match state.overlays.reissue_all_credentials().await {
            Ok(reissued) => reissued,
            Err(reissue_error) => {
                return match gate.abandon_rotation(state.credentials.as_ref()).await {
                    Ok(()) => Err(reissue_error.into()),
                    Err(restore_error) => {
                        tracing::error!(error = %restore_error, "could not restore the previous overlay token after an aborted rotation");
                        gate.commit_rotation(fresh.clone());
                        Err(ServerError::OverlayTokenRestoreFailed {
                            token: SecretToken::new(fresh),
                            reason: format!("{reissue_error}; {restore_error}"),
                        })
                    }
                };
            }
        };
        gate.commit_rotation(fresh.clone());
        for identity in &reissued {
            state.bus_adapter.revoke_overlay(identity).await;
        }
        let incomplete = |reason: String| ServerError::OverlayRotationIncomplete {
            token: SecretToken::new(fresh.clone()),
            reason,
        };
        let pass = overlay_service
            .materialize_all()
            .await
            .map_err(|e| incomplete(e.to_string()))?;
        if pass.failed > 0 {
            return Err(incomplete(format!(
                "{} overlay pages failed to rewrite",
                pass.failed
            )));
        }
        Ok(fresh)
    }

    pub async fn bind_addr(&self) -> SocketAddr {
        self.inner.lock().await.bind_addr
    }

    pub async fn overlay_root(&self) -> Arc<std::path::PathBuf> {
        Arc::clone(&self.inner.lock().await.state.overlay_root)
    }

    pub async fn offer_audio_clip(
        &self,
        owner: &OverlayId,
        offer: ClipOffer,
    ) -> Result<(ClipTicket, ClipOutcomeHandle), ServerError> {
        let store = Arc::clone(&self.inner.lock().await.state.audio_clips);
        store.offer(owner, offer)
    }

    pub async fn revoke_audio_clip(&self, capability: &ClipCapability) -> bool {
        let store = Arc::clone(&self.inner.lock().await.state.audio_clips);
        store.revoke(capability.expose())
    }

    pub async fn admit_audio_players(&self, capability: &ClipCapability, players: usize) -> bool {
        let store = Arc::clone(&self.inner.lock().await.state.audio_clips);
        store.admit_players(capability.expose(), players)
    }

    pub async fn hold_audio_clip(&self, capability: &ClipCapability) -> bool {
        let store = Arc::clone(&self.inner.lock().await.state.audio_clips);
        store.hold(capability.expose())
    }

    pub async fn release_audio_clip(&self, capability: &ClipCapability) -> bool {
        let store = Arc::clone(&self.inner.lock().await.state.audio_clips);
        store.release_hold(capability.expose())
    }

    pub async fn deliver_overlay_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> OverlayReceivers {
        let adapter = Arc::clone(&self.inner.lock().await.state.bus_adapter);
        adapter
            .deliver_overlay_content(identity, &content, duration_ms)
            .await
    }

    pub async fn overlay_receivers(&self, identity: &OverlayId) -> OverlayReceivers {
        let adapter = Arc::clone(&self.inner.lock().await.state.bus_adapter);
        adapter.overlay_receivers(identity).await
    }

    pub async fn set_overlay_connect_listener(&self, listener: Arc<dyn OverlayConnectListener>) {
        let adapter = Arc::clone(&self.inner.lock().await.state.bus_adapter);
        adapter.set_overlay_connect_listener(listener);
    }

    pub async fn deliver_overlay_reload(&self, identity: Option<&OverlayId>) {
        let adapter = Arc::clone(&self.inner.lock().await.state.bus_adapter);
        adapter.deliver_overlay_reload(identity).await;
    }

    pub async fn revoke_overlay(&self, identity: &OverlayId) -> usize {
        let adapter = Arc::clone(&self.inner.lock().await.state.bus_adapter);
        adapter.revoke_overlay(identity).await
    }

    pub async fn snapshot(&self) -> crate::snapshot::ServerSnapshot {
        let state = self.inner.lock().await.state.clone();
        crate::snapshot::build_server_snapshot(&state.server_info, &state.bus_adapter).await
    }

    pub async fn kick_client(&self, identification: &str) -> bool {
        let state = self.inner.lock().await.state.clone();
        let id = {
            let clients = state.server_info.connected_clients.read().await;
            clients
                .iter()
                .find(|(_, client)| client.identification.load_full().as_str() == identification)
                .map(|(id, _)| *id)
        };
        let Some(id) = id else {
            return false;
        };
        state.server_info.unregister(id).await;
        state.bus_adapter.kick_client(id).await;
        true
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::HashSet;

    use forge_registry::SubActionRegistry;
    use forge_runtime::{ActionCancelRegistry, EventBus, NullEventLogRepo, spawn_action_engine};
    use forge_storage::credentials::MockCredentialsRepo;
    use forge_storage::{
        CredentialsRepo, DataProvider, GlobalsRepo, SettingsRepo, UserGlobalsRepo,
    };

    use super::*;
    use crate::bus_adapter::{BusAdapter, ClientFilterSet, OverlayClientClass};
    use crate::config::ServerConfig;
    use crate::server::stopped_server;
    use crate::test_helpers::{MemSettings, TestDataProvider};

    async fn stopped_handle() -> ServerHandle {
        let dp = Arc::new(TestDataProvider::new());
        let mut credentials = MockCredentialsRepo::new();
        credentials.expect_load().returning(|_| Ok(None));
        credentials.expect_store().returning(|_, _| Ok(()));
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            forge_runtime::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        ));
        let config = ServerConfig::new(
            MemSettings::new() as Arc<dyn SettingsRepo>,
            Arc::new(credentials) as Arc<dyn CredentialsRepo>,
            bus,
            dp.action_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            dp.overlay_repo(),
            engine,
        );

        stopped_server(config).await.expect("a stopped handle")
    }

    type CredentialSlots = Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>;

    fn slot_credentials() -> (MockCredentialsRepo, CredentialSlots) {
        let slots: CredentialSlots = Arc::default();
        let mut credentials = MockCredentialsRepo::new();
        let read = Arc::clone(&slots);
        credentials
            .expect_load()
            .returning(move |id| Ok(read.lock().expect("lock").get(id.as_str()).cloned()));
        let write = Arc::clone(&slots);
        credentials.expect_store().returning(move |id, value| {
            write
                .lock()
                .expect("lock")
                .insert(id.as_str().to_owned(), value.to_owned());
            Ok(())
        });
        (credentials, slots)
    }

    async fn rotation_handle(
        reissue: Result<Vec<OverlayId>, forge_storage::StorageError>,
    ) -> (ServerHandle, CredentialSlots) {
        let mut dp = TestDataProvider::new();
        let reissue = std::sync::Mutex::new(Some(reissue));
        dp.overlay()
            .expect_reissue_all_credentials()
            .returning(move || reissue.lock().expect("lock").take().expect("one reissue"));
        let dp = Arc::new(dp);
        let (credentials, slots) = slot_credentials();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = Arc::new(spawn_action_engine(
            Arc::clone(&bus),
            forge_runtime::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        ));
        let config = ServerConfig::new(
            MemSettings::new() as Arc<dyn SettingsRepo>,
            Arc::new(credentials) as Arc<dyn CredentialsRepo>,
            bus,
            dp.action_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            dp.overlay_repo(),
            engine,
        );
        (
            stopped_server(config).await.expect("a stopped handle"),
            slots,
        )
    }

    async fn empty_overlay_service(root: &std::path::Path) -> OverlayServiceHandle {
        let mut repo = forge_storage::MockOverlayRepo::new();
        repo.expect_list().returning(|| Ok(Vec::new()));
        let settings = MemSettings::new();
        settings
            .set_string(
                forge_storage::reserved_keys::SERVER_OVERLAY_ROOT,
                &root.to_string_lossy(),
            )
            .await
            .expect("overlay root setting");
        OverlayServiceHandle::new(
            Arc::new(repo),
            settings as Arc<dyn SettingsRepo>,
            Arc::new(forge_overlay::OverlayKindRegistry::new()),
            EventBus::new(Arc::new(NullEventLogRepo)),
            None,
        )
    }

    async fn live_overlay_page(
        handle: &ServerHandle,
        identity: &OverlayId,
    ) -> tokio::sync::broadcast::Receiver<crate::bus_adapter::WsFrame> {
        let adapter = adapter_of(handle).await;
        let (client, _initial_rx) = adapter
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        adapter
            .promote_to_overlay(
                client.id,
                identity.clone(),
                OverlayClientClass::BrowserSource,
            )
            .await
            .expect("a just-registered client is still on the registry")
    }

    fn received_close(
        page: &mut tokio::sync::broadcast::Receiver<crate::bus_adapter::WsFrame>,
    ) -> bool {
        std::iter::from_fn(|| page.try_recv().ok())
            .any(|frame| matches!(frame, crate::bus_adapter::WsFrame::Close))
    }

    #[tokio::test]
    async fn regenerating_the_overlay_token_refuses_the_old_token_and_persists_the_new_one() {
        let root = tempfile::tempdir().expect("tempdir");
        let (handle, slots) = rotation_handle(Ok(Vec::new())).await;
        let gate_before = handle.auth_state().await;
        let old = gate_before.overlay_token().current();

        let fresh = handle
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await)
            .await
            .expect("regenerate");

        let gate = handle.auth_state().await;
        assert!(
            !gate.overlay_token().verify(&old),
            "the old token still opens overlay files"
        );
        assert!(gate.overlay_token().verify(&fresh));
        assert_eq!(
            slots
                .lock()
                .expect("lock")
                .get(forge_storage::SERVER_OVERLAY_TOKEN_CREDENTIAL_ID)
                .map(String::as_str),
            Some(fresh.as_str()),
            "a restart would bring the old token back"
        );
    }

    #[tokio::test]
    async fn regenerating_the_overlay_token_closes_the_live_page_of_every_reissued_overlay() {
        let root = tempfile::tempdir().expect("tempdir");
        let alerts = OverlayId::new("alerts");
        let (handle, _slots) = rotation_handle(Ok(vec![alerts.clone()])).await;
        let mut page = live_overlay_page(&handle, &alerts).await;

        handle
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await)
            .await
            .expect("regenerate");

        assert!(
            received_close(&mut page),
            "a page holding a reissued credential keeps streaming after rotation"
        );
    }

    #[tokio::test]
    async fn a_failed_credential_reissue_keeps_the_old_token_live_stored_and_its_pages_open() {
        let root = tempfile::tempdir().expect("tempdir");
        let alerts = OverlayId::new("alerts");
        let (handle, slots) = rotation_handle(Err(forge_storage::StorageError::Connection {
            reason: "database is locked".into(),
        }))
        .await;
        let old = handle.auth_state().await.overlay_token().current();
        let mut page = live_overlay_page(&handle, &alerts).await;

        let result = handle
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await)
            .await;

        assert!(result.is_err());
        assert!(handle.auth_state().await.overlay_token().verify(&old));
        assert_eq!(
            stored_overlay_token(&slots),
            Some(old),
            "a restart would load a token no URL carries"
        );
        assert!(!received_close(&mut page));
    }

    #[tokio::test]
    async fn a_failed_page_rewrite_reports_the_live_token_and_refuses_the_old_one() {
        let root = tempfile::tempdir().expect("tempdir");
        let not_a_directory = root.path().join("overlays-file");
        tokio::fs::write(&not_a_directory, b"x")
            .await
            .expect("write file");
        let (handle, slots) = rotation_handle(Ok(Vec::new())).await;
        let old = handle.auth_state().await.overlay_token().current();

        let result = handle
            .regenerate_overlay_token(&empty_overlay_service(&not_a_directory).await)
            .await;

        let error = result.expect_err("a failed rewrite must fail the regenerate");
        let ServerError::OverlayRotationIncomplete { token, .. } = &error else {
            panic!("expected an incomplete rotation, got {error:?}");
        };
        let gate = handle.auth_state().await;
        assert!(
            gate.overlay_token().verify(token),
            "the reported token is not the live one"
        );
        assert!(!gate.overlay_token().verify(&old));
        assert_eq!(
            stored_overlay_token(&slots).as_deref(),
            Some(token.as_str())
        );
        assert!(
            !error.to_string().contains(token.as_str()),
            "the toast text carries the overlay token"
        );
    }

    fn stored_overlay_token(slots: &CredentialSlots) -> Option<String> {
        slots
            .lock()
            .expect("lock")
            .get(forge_storage::SERVER_OVERLAY_TOKEN_CREDENTIAL_ID)
            .cloned()
    }

    async fn adapter_of(handle: &ServerHandle) -> Arc<BusAdapter> {
        Arc::clone(&handle.inner.lock().await.state.bus_adapter)
    }

    #[tokio::test]
    async fn the_handle_counts_the_pages_its_registry_holds_for_the_overlay_asked() {
        let handle = stopped_handle().await;
        let target = OverlayId::new("stage-audio");
        let adapter = adapter_of(&handle).await;
        let (client, _initial_rx) = adapter
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        let _page = adapter
            .promote_to_overlay(client.id, target.clone(), OverlayClientClass::BrowserSource)
            .await
            .expect("a just-registered client is still on the registry");

        assert_eq!(
            handle.overlay_receivers(&target).await,
            OverlayReceivers {
                sources: 1,
                preview_tabs: 0
            },
            "the handle answered from somewhere other than the live registry"
        );
        assert_eq!(
            handle.overlay_receivers(&OverlayId::new("alert-box")).await,
            OverlayReceivers::default(),
            "the handle credited one overlay with another overlay's pages"
        );
    }
}
