use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_runtime::{OverlaySampleSource, OverlayServiceHandle};
use forge_storage::OverlayId;
use forge_types::{ClipCapability, Secret};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, watch};

use crate::audio_clips::{ClipOffer, ClipOutcomeHandle, ClipTicket};
use crate::auth::AuthState;
use crate::bus_adapter::ClientId;
use crate::config::ServerSettings;
use crate::origin::build_allowed_origins;
use crate::run_state::{Generation, RunState, ServerRunWatch};
use crate::server::AppState;
use crate::{ServerError, server};

pub(crate) const CONNECTION_DRAIN_BUDGET: Duration = Duration::from_secs(5);

struct HandleInner {
    join: Option<tokio::task::JoinHandle<Result<(), ServerError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    state: AppState,
    bind_addr: SocketAddr,
}

#[derive(Clone)]
pub struct ServerHandle {
    inner: Arc<Mutex<HandleInner>>,
    lifecycle: Arc<Mutex<()>>,
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
            lifecycle: Arc::new(Mutex::new(())),
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
            lifecycle: Arc::new(Mutex::new(())),
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

    pub fn run_state(&self) -> ServerRunWatch {
        self.run_state.subscribe()
    }

    pub async fn stop(&self) -> Result<(), ServerError> {
        let _lifecycle = self.lifecycle.lock().await;
        self.stop_serving().await
    }

    async fn stop_serving(&self) -> Result<(), ServerError> {
        let (shutdown_tx, join, bus_adapter, drain_budget) = {
            let mut guard = self.inner.lock().await;

            if guard.shutdown_tx.is_none() {
                return Ok(());
            }

            guard.state.audio_clips.discard_all();

            (
                guard.shutdown_tx.take(),
                guard.join.take(),
                Arc::clone(&guard.state.bus_adapter),
                guard.state.timing.connection_drain_budget,
            )
        };

        if let Some(tx) = shutdown_tx {
            tx.send_replace(true);
        }

        self.run_state.stop();

        bus_adapter.broadcast_close().await;

        let drain = async {
            let Some(join) = join else {
                return;
            };
            match join.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "server stopped with an error"),
                Err(error) => tracing::warn!(%error, "server task ended abnormally"),
            }
        };

        if tokio::time::timeout(drain_budget, drain).await.is_err() {
            tracing::warn!(budget = ?drain_budget, "server connections did not drain in time");
        }

        Ok(())
    }

    pub async fn restart(&self) -> Result<(), ServerError> {
        let _lifecycle = self.lifecycle.lock().await;
        let state = {
            let guard = self.inner.lock().await;
            guard.state.clone()
        };

        let settings = ServerSettings::load(state.settings.as_ref())
            .await
            .map_err(ServerError::Storage)?;

        if !settings.enabled {
            self.stop_serving().await?;
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
            .unwrap_or_else(forge_paths::overlays_dir);

        server::validate_lan_bind(&bind_addr, settings.lan_bind_enabled)?;

        self.stop_serving().await?;
        state
            .auth
            .set_reads_required(settings.auth_required_for_reads);
        state
            .auth
            .set_lan_exposed(server::is_lan_bind(bind_addr.ip()));
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
            timing: state.timing,
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

    pub(crate) async fn auth_state(&self) -> Arc<AuthState> {
        Arc::clone(&self.inner.lock().await.state.auth)
    }

    pub async fn bearer_token(&self) -> Secret {
        self.auth_state().await.current_token().await
    }

    pub async fn regenerate_bearer_token(&self) -> Result<Secret, ServerError> {
        let state = self.inner.lock().await.state.clone();
        state.auth.regenerate(state.credentials.as_ref()).await
    }

    pub async fn reads_require_auth(&self) -> bool {
        self.auth_state().await.reads_required()
    }

    pub async fn set_reads_require_auth(&self, required: bool) -> Result<(), ServerError> {
        let state = self.inner.lock().await.state.clone();
        ServerSettings::save_auth_required_for_reads(state.settings.as_ref(), required).await?;
        state.auth.set_reads_required(required);
        Ok(())
    }

    pub async fn overlay_token(&self) -> Secret {
        self.auth_state().await.overlay_token().current()
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
        samples: &dyn OverlaySampleSource,
    ) -> Result<Secret, ServerError> {
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
                            token: fresh,
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
            token: fresh.clone(),
            reason,
        };
        let pass = overlay_service
            .materialize_all(samples)
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

    pub async fn snapshot(&self) -> crate::snapshot::ServerSnapshot {
        let state = self.inner.lock().await.state.clone();
        crate::snapshot::build_server_snapshot(&state.server_info, &state.bus_adapter).await
    }

    pub async fn kick_client(&self, id: ClientId) -> bool {
        let state = self.inner.lock().await.state.clone();
        let connected = state
            .server_info
            .connected_clients
            .read()
            .await
            .contains_key(&id);
        if !connected {
            return false;
        }
        state.server_info.unregister(id).await;
        state.bus_adapter.kick_client(id).await;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use forge_runtime::{EventBus, NullEventLogRepo, OverlayServiceParts};
    use forge_storage::{CredentialsRepo, SettingsRepo};

    use super::*;
    use crate::bus_adapter::{BusAdapter, ClientFilterSet, OverlayClientClass};
    use crate::delivery_hub::DeliveryHub;
    use crate::server::stopped_server;
    use crate::test_helpers::{MemCreds, TestDataProvider, mem_settings, server_config, test_dp};
    use forge_storage::test_support::INJECTED_FAILURE_REASON;

    async fn stopped_handle() -> ServerHandle {
        stopped_handle_over(mem_settings() as Arc<dyn SettingsRepo>, MemCreds::new()).await
    }

    async fn stopped_handle_over(
        settings: Arc<dyn SettingsRepo>,
        credentials: Arc<MemCreds>,
    ) -> ServerHandle {
        stopped_server(server_config(settings, credentials, test_dp()))
            .await
            .expect("a stopped handle")
    }

    const PREVIOUS_OVERLAY_TOKEN: &str = "fgo_previous-live-token";

    fn credentials_refusing_to_restore(previous: &str) -> Arc<MemCreds> {
        let credentials =
            MemCreds::with_slot(forge_storage::SERVER_OVERLAY_TOKEN_CREDENTIAL_ID, previous);
        credentials.refuse_writes_of(previous);
        credentials
    }

    async fn rotation_handle(
        reissue: Result<Vec<OverlayId>, forge_storage::StorageError>,
    ) -> (ServerHandle, Arc<MemCreds>) {
        rotation_handle_over(reissue, MemCreds::new()).await
    }

    async fn rotation_handle_over(
        reissue: Result<Vec<OverlayId>, forge_storage::StorageError>,
        credentials: Arc<MemCreds>,
    ) -> (ServerHandle, Arc<MemCreds>) {
        let mut dp = TestDataProvider::new();
        let reissue = std::sync::Mutex::new(Some(reissue));
        dp.overlay()
            .expect_reissue_all_credentials()
            .returning(move || reissue.lock().expect("lock").take().expect("one reissue"));
        let config = server_config(
            mem_settings() as Arc<dyn SettingsRepo>,
            Arc::clone(&credentials) as Arc<dyn CredentialsRepo>,
            Arc::new(dp),
        );
        (
            stopped_server(config).await.expect("a stopped handle"),
            credentials,
        )
    }

    struct NeutralSamples;

    #[async_trait::async_trait]
    impl OverlaySampleSource for NeutralSamples {
        async fn sample(&self, _: &OverlayId) -> forge_overlay::SampleContext {
            forge_overlay::SampleContext::neutral()
        }
    }

    async fn empty_overlay_service(root: &std::path::Path) -> OverlayServiceHandle {
        let mut repo = forge_storage::MockOverlayRepo::new();
        repo.expect_list().returning(|| Ok(Vec::new()));
        let settings = mem_settings();
        settings
            .seed(
                &forge_storage::settings::core_settings::SERVER_OVERLAY_ROOT,
                &Some(root.to_string_lossy().into_owned()),
            )
            .expect("overlay root setting");
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        OverlayServiceHandle::new(OverlayServiceParts {
            repo: Arc::new(repo),
            settings: settings as Arc<dyn SettingsRepo>,
            kinds: Arc::new(forge_overlay::OverlayKindRegistry::new()),
            bus: Arc::clone(&bus),
            frames: Arc::new(DeliveryHub::new(bus)),
            media: None,
            speaker: None,
            latest: None,
        })
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
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await, &NeutralSamples)
            .await
            .expect("regenerate");

        let gate = handle.auth_state().await;
        assert!(
            !gate.overlay_token().verify(old.expose()),
            "the old token still opens overlay files"
        );
        assert!(gate.overlay_token().verify(fresh.expose()));
        assert_eq!(
            stored_overlay_token(&slots).as_deref(),
            Some(fresh.expose()),
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
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await, &NeutralSamples)
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
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await, &NeutralSamples)
            .await;

        assert!(
            matches!(&result, Err(ServerError::Storage(reason)) if reason.to_string().contains("database is locked")),
            "a restored rotation must report the reissue failure itself: {result:?}"
        );
        assert!(
            handle
                .auth_state()
                .await
                .overlay_token()
                .verify(old.expose())
        );
        assert_eq!(
            stored_overlay_token(&slots).as_deref(),
            Some(old.expose()),
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
            .regenerate_overlay_token(
                &empty_overlay_service(&not_a_directory).await,
                &NeutralSamples,
            )
            .await;

        let error = result.expect_err("a failed rewrite must fail the regenerate");
        let ServerError::OverlayRotationIncomplete { token, .. } = &error else {
            panic!("expected an incomplete rotation, got {error:?}");
        };
        let gate = handle.auth_state().await;
        assert!(
            gate.overlay_token().verify(token.expose()),
            "the reported token is not the live one"
        );
        assert!(!gate.overlay_token().verify(old.expose()));
        assert_eq!(
            stored_overlay_token(&slots).as_deref(),
            Some(token.expose())
        );
        assert!(
            !error.to_string().contains(token.expose()),
            "the toast text carries the overlay token"
        );
    }

    async fn regenerate_with_a_failed_reissue_and_restore(
        root: &std::path::Path,
    ) -> (ServerHandle, Arc<MemCreds>, Result<Secret, ServerError>) {
        let (handle, slots) = rotation_handle_over(
            Err(forge_storage::StorageError::Connection {
                reason: "database is locked".into(),
            }),
            credentials_refusing_to_restore(PREVIOUS_OVERLAY_TOKEN),
        )
        .await;
        let result = handle
            .regenerate_overlay_token(&empty_overlay_service(root).await, &NeutralSamples)
            .await;
        (handle, slots, result)
    }

    #[tokio::test]
    async fn a_failed_restore_adopts_the_stored_token_live_and_reports_it() {
        let root = tempfile::tempdir().expect("tempdir");

        let (handle, slots, result) =
            regenerate_with_a_failed_reissue_and_restore(root.path()).await;

        let error = result.expect_err("a failed reissue must fail the regenerate");
        let ServerError::OverlayTokenRestoreFailed { token, .. } = &error else {
            panic!("expected a failed restore, got {error:?}");
        };
        let gate = handle.auth_state().await;
        assert!(
            gate.overlay_token().verify(token.expose()),
            "the reported token is not the live one"
        );
        assert_eq!(
            stored_overlay_token(&slots).as_deref(),
            Some(token.expose()),
            "a restart would load a token no URL carries"
        );
    }

    #[tokio::test]
    async fn a_failed_restore_refuses_the_previous_token() {
        let root = tempfile::tempdir().expect("tempdir");

        let (handle, _slots, result) =
            regenerate_with_a_failed_reissue_and_restore(root.path()).await;

        assert!(result.is_err());
        assert!(
            !handle
                .auth_state()
                .await
                .overlay_token()
                .verify(PREVIOUS_OVERLAY_TOKEN),
            "the live gate still admits a token the store no longer holds"
        );
    }

    #[tokio::test]
    async fn a_failed_restore_message_never_carries_the_new_token() {
        let root = tempfile::tempdir().expect("tempdir");

        let (_handle, _slots, result) =
            regenerate_with_a_failed_reissue_and_restore(root.path()).await;

        let error = result.expect_err("a failed reissue must fail the regenerate");
        let ServerError::OverlayTokenRestoreFailed { token, reason } = &error else {
            panic!("expected a failed restore, got {error:?}");
        };
        assert!(
            !error.to_string().contains(token.expose()) && !reason.contains(token.expose()),
            "the toast text carries the overlay token"
        );
    }

    fn stored_overlay_token(credentials: &MemCreds) -> Option<String> {
        credentials.stored(forge_storage::SERVER_OVERLAY_TOKEN_CREDENTIAL_ID)
    }

    async fn adapter_of(handle: &ServerHandle) -> Arc<BusAdapter> {
        Arc::clone(&handle.inner.lock().await.state.bus_adapter)
    }

    #[tokio::test]
    async fn regenerating_the_bearer_token_makes_the_new_token_live_and_stored_and_refuses_the_old_one()
     {
        let slots = MemCreds::new();
        let handle =
            stopped_handle_over(mem_settings() as Arc<dyn SettingsRepo>, Arc::clone(&slots)).await;
        let old = handle.bearer_token().await;

        let fresh = handle.regenerate_bearer_token().await.expect("regenerate");

        assert_ne!(fresh, old);
        assert_eq!(handle.bearer_token().await, fresh);
        assert_eq!(
            slots
                .stored(forge_storage::SERVER_BEARER_CREDENTIAL_ID)
                .as_deref(),
            Some(fresh.expose()),
            "a restart would bring the old token back"
        );
        let gate = handle.auth_state().await;
        assert!(
            !gate.verify(old.expose()).await,
            "the old bearer still authenticates"
        );
        assert!(gate.verify(fresh.expose()).await);
    }

    #[tokio::test]
    async fn a_bearer_regenerate_the_store_refuses_keeps_the_old_token_live() {
        let slots = MemCreds::new();
        let handle =
            stopped_handle_over(mem_settings() as Arc<dyn SettingsRepo>, Arc::clone(&slots)).await;
        let old = handle.bearer_token().await;
        slots.refuse_every_write(true);

        let result = handle.regenerate_bearer_token().await;

        assert!(
            matches!(&result, Err(ServerError::Storage(reason)) if reason.to_string().contains("disk is full")),
            "{result:?}"
        );
        assert_eq!(handle.bearer_token().await, old);
        assert!(handle.auth_state().await.verify(old.expose()).await);
        assert_eq!(
            slots
                .stored(forge_storage::SERVER_BEARER_CREDENTIAL_ID)
                .as_deref(),
            Some(old.expose())
        );
    }

    #[tokio::test]
    async fn the_reads_switch_is_stored_and_applied_live_in_both_directions() {
        let settings = mem_settings();
        let handle = stopped_handle_over(
            Arc::clone(&settings) as Arc<dyn SettingsRepo>,
            MemCreds::new(),
        )
        .await;

        for required in [true, false] {
            handle
                .set_reads_require_auth(required)
                .await
                .expect("set reads switch");

            assert_eq!(handle.reads_require_auth().await, required);
            let stored = ServerSettings::load(settings.as_ref())
                .await
                .expect("load settings");
            assert_eq!(
                stored.auth_required_for_reads, required,
                "a restart would revert the reads switch"
            );
        }
    }

    #[tokio::test]
    async fn a_reads_switch_the_settings_store_refuses_leaves_the_live_switch_unchanged() {
        let settings = mem_settings();
        settings.fail_writes(true);
        let handle = stopped_handle_over(settings as Arc<dyn SettingsRepo>, MemCreds::new()).await;
        let before = handle.reads_require_auth().await;

        let result = handle.set_reads_require_auth(!before).await;

        assert!(
            matches!(&result, Err(ServerError::Storage(reason)) if reason.to_string().contains(INJECTED_FAILURE_REASON)),
            "{result:?}"
        );
        assert_eq!(handle.reads_require_auth().await, before);
    }

    #[tokio::test]
    async fn the_handed_out_overlay_token_is_the_one_the_gate_admits_across_a_rotation() {
        let root = tempfile::tempdir().expect("tempdir");
        let (handle, _slots) = rotation_handle(Ok(Vec::new())).await;
        let before = handle.overlay_token().await;
        assert!(
            handle
                .auth_state()
                .await
                .overlay_token()
                .verify(before.expose())
        );

        let fresh = handle
            .regenerate_overlay_token(&empty_overlay_service(root.path()).await, &NeutralSamples)
            .await
            .expect("regenerate");

        assert_eq!(handle.overlay_token().await, fresh);
    }

    #[tokio::test]
    async fn tokens_handed_out_by_the_handle_never_print_in_debug() {
        let handle = stopped_handle().await;
        let regenerated = handle.regenerate_bearer_token().await.expect("regenerate");

        for token in [
            handle.bearer_token().await,
            regenerated,
            handle.overlay_token().await,
        ] {
            let debug = format!("{token:?}");
            assert!(!debug.contains(token.expose()), "token leaked: {debug}");
        }
    }
}
