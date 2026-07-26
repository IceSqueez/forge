use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use forge_events::EventPublisher;
use forge_platform_core::{
    AtomicConnectionState, Backoff, BuiltinControl, BuiltinId, BuiltinStatus, CapabilityFlags,
    ConnectionState, ControlFailure, ControlOutcome, HeaderAction, HealthDelta, HeroBadge,
    HeroBadgeTone,
};
use forge_types::EventId;

use crate::catalog::ObsCatalog;
use crate::error::ObsError;
use crate::health::{HealthSnapshot, make_health_channel};
use crate::source::SourceInfo;

pub struct ObsClient {
    pub(crate) inner: Arc<tokio::sync::RwLock<Option<Arc<obws::Client>>>>,
    pub(crate) scene_item_id_cache: Arc<Mutex<HashMap<(String, String), i64>>>,
    pub(crate) last_set_scene_event_id: Arc<RwLock<Option<EventId>>>,
    endpoint: String,
    state: Arc<AtomicConnectionState>,
    // async Mutex: reconnect swaps the Notify for a new supervisor cycle without racing
    // the running supervisor's own clone of the Arc.
    shutdown: Arc<tokio::sync::Mutex<Arc<Notify>>>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    obs_id: BuiltinId,
    obs_version: Arc<OnceLock<String>>,
    obs_ws_version: Arc<OnceLock<String>>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) catalog_state: Arc<RwLock<ObsCatalog>>,
    reconnect_host: String,
    reconnect_port: u16,
    // Never logged or surfaced.
    reconnect_password: Arc<Option<String>>,
    reconnect_publisher: Arc<dyn EventPublisher>,
    auto_reconnect: Arc<AtomicBool>,
}

impl ObsClient {
    pub async fn connect(
        endpoint: &str,
        password: Option<&str>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Self, ObsError> {
        let (host, port) = parse_endpoint(endpoint)?;

        let inner = Arc::new(tokio::sync::RwLock::new(None::<Arc<obws::Client>>));
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connecting));
        let notify = Arc::new(Notify::new());
        let shutdown = Arc::new(tokio::sync::Mutex::new(Arc::clone(&notify)));
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let obs_version = Arc::new(OnceLock::new());
        let obs_ws_version = Arc::new(OnceLock::new());
        let item_cache = Arc::new(Mutex::new(HashMap::<(String, String), i64>::new()));

        let (health_tx, health_state) = make_health_channel();
        let catalog_state = Arc::new(RwLock::new(ObsCatalog::default()));
        let last_set_scene_event_id = Arc::new(RwLock::new(None::<EventId>));

        let stored_password = password.map(str::to_owned);
        let auto_reconnect = Arc::new(AtomicBool::new(true));

        let ctx = SupervisorContext {
            inner: Arc::clone(&inner),
            state: Arc::clone(&state),
            shutdown: Arc::clone(&notify),
            connected_at: Arc::clone(&connected_at),
            obs_version: Arc::clone(&obs_version),
            obs_ws_version: Arc::clone(&obs_ws_version),
            catalog_state: Arc::clone(&catalog_state),
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            publisher: Arc::clone(&publisher),
            item_cache: Arc::clone(&item_cache),
            last_set_scene_event_id: Arc::clone(&last_set_scene_event_id),
            auto_reconnect: Arc::clone(&auto_reconnect),
        };
        let handle = tokio::spawn(run_supervisor(
            host.clone(),
            port,
            stored_password.clone(),
            ctx,
        ));

        Ok(Self {
            inner,
            endpoint: endpoint.to_owned(),
            state,
            shutdown,
            supervisor: Arc::new(std::sync::Mutex::new(Some(handle))),
            connected_at,
            obs_id: BuiltinId::new("obs"),
            obs_version,
            obs_ws_version,
            health_state,
            health_tx,
            catalog_state,
            scene_item_id_cache: item_cache,
            last_set_scene_event_id,
            reconnect_host: host,
            reconnect_port: port,
            reconnect_password: Arc::new(stored_password),
            reconnect_publisher: publisher,
            auto_reconnect,
        })
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state.load()
    }

    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.auto_reconnect.store(enabled, Ordering::Relaxed);
    }

    pub fn auto_reconnect_enabled(&self) -> bool {
        self.auto_reconnect.load(Ordering::Relaxed)
    }

    pub(crate) async fn active_client(&self) -> Result<Arc<obws::Client>, ObsError> {
        let guard = self.inner.read().await;
        guard.clone().ok_or(ObsError::Disconnected)
    }

    #[cfg(test)]
    pub fn new_for_test(endpoint: String) -> Self {
        let (host, port) = parse_endpoint(&endpoint).unwrap_or(("localhost".to_owned(), 4455));
        let (health_tx, health_state) = make_health_channel();
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(None)),
            endpoint,
            state: Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected)),
            shutdown: Arc::new(tokio::sync::Mutex::new(Arc::new(Notify::new()))),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            obs_id: BuiltinId::new("obs"),
            obs_version: Arc::new(OnceLock::new()),
            obs_ws_version: Arc::new(OnceLock::new()),
            health_state,
            health_tx,
            catalog_state: Arc::new(RwLock::new(ObsCatalog::default())),
            scene_item_id_cache: Arc::new(Mutex::new(HashMap::new())),
            last_set_scene_event_id: Arc::new(RwLock::new(None)),
            reconnect_host: host,
            reconnect_port: port,
            reconnect_password: Arc::new(None),
            reconnect_publisher: Arc::new(crate::runners::test_support::NoopPublisher),
            auto_reconnect: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl BuiltinStatus for ObsClient {
    fn id(&self) -> &BuiltinId {
        &self.obs_id
    }

    fn display_name(&self) -> &str {
        "OBS Studio"
    }

    fn version(&self) -> Option<&str> {
        self.obs_version.get().map(|s| s.as_str())
    }

    fn connection(&self) -> ConnectionState {
        self.connection_state()
    }

    fn uptime(&self) -> Option<Duration> {
        let at = {
            let guard = self.connected_at.read().ok()?;
            *guard
        }?;
        let elapsed = OffsetDateTime::now_utc() - at;
        if elapsed.is_positive() {
            Some(elapsed.unsigned_abs())
        } else {
            None
        }
    }

    fn endpoint(&self) -> Option<&str> {
        Some(&self.endpoint)
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::Reconnect, HeaderAction::Settings]
    }

    /// `obs-websocket` protocol version plus our own WS session uptime, computed fresh on every
    /// render (unlike `endpoint()`/`version()`, which are fixed `&str` slots that cannot carry a
    /// value that changes every tick without violating their `&self`-tied lifetime).
    fn name_badges(&self) -> Vec<HeroBadge> {
        let Some(ws_version) = self.obs_ws_version.get() else {
            return Vec::new();
        };
        let label = match self.uptime() {
            Some(uptime) => format!(
                "obs-websocket v{ws_version} - uptime {}",
                crate::health::format_duration_hm(uptime)
            ),
            None => format!("obs-websocket v{ws_version}"),
        };
        vec![HeroBadge {
            label,
            tone: HeroBadgeTone::Neutral,
            monospace: true,
        }]
    }
}

#[async_trait]
impl BuiltinControl for ObsClient {
    async fn reconnect(&self) -> ControlOutcome {
        // Locking `shutdown` serialises concurrent reconnect/disconnect calls.
        let mut slot = self.shutdown.lock().await;
        let old_notify = slot.clone();
        old_notify.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }

        let new_notify = Arc::new(Notify::new());
        *slot = Arc::clone(&new_notify);
        drop(slot);

        self.state.store(ConnectionState::Connecting);
        clear_connected_at(&self.connected_at);

        let ctx = SupervisorContext {
            inner: Arc::clone(&self.inner),
            state: Arc::clone(&self.state),
            shutdown: new_notify,
            connected_at: Arc::clone(&self.connected_at),
            obs_version: Arc::clone(&self.obs_version),
            obs_ws_version: Arc::clone(&self.obs_ws_version),
            catalog_state: Arc::clone(&self.catalog_state),
            health_state: Arc::clone(&self.health_state),
            health_tx: self.health_tx.clone(),
            publisher: Arc::clone(&self.reconnect_publisher),
            item_cache: Arc::clone(&self.scene_item_id_cache),
            last_set_scene_event_id: Arc::clone(&self.last_set_scene_event_id),
            auto_reconnect: Arc::clone(&self.auto_reconnect),
        };
        let password = (*self.reconnect_password).clone();
        let handle = tokio::spawn(run_supervisor(
            self.reconnect_host.clone(),
            self.reconnect_port,
            password,
            ctx,
        ));
        if let Ok(mut g) = self.supervisor.lock() {
            *g = Some(handle);
        }

        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let slot = self.shutdown.lock().await;
        let notify = slot.clone();
        drop(slot);
        notify.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }
        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}

struct SupervisorContext {
    inner: Arc<tokio::sync::RwLock<Option<Arc<obws::Client>>>>,
    state: Arc<AtomicConnectionState>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    obs_version: Arc<OnceLock<String>>,
    obs_ws_version: Arc<OnceLock<String>>,
    catalog_state: Arc<RwLock<ObsCatalog>>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    publisher: Arc<dyn EventPublisher>,
    item_cache: Arc<Mutex<HashMap<(String, String), i64>>>,
    last_set_scene_event_id: Arc<RwLock<Option<EventId>>>,
    auto_reconnect: Arc<AtomicBool>,
}

const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

async fn run_supervisor(host: String, port: u16, password: Option<String>, ctx: SupervisorContext) {
    let SupervisorContext {
        inner,
        state,
        shutdown,
        connected_at,
        obs_version,
        obs_ws_version,
        catalog_state,
        health_state,
        health_tx,
        publisher,
        item_cache,
        last_set_scene_event_id,
        auto_reconnect,
    } = ctx;
    let mut backoff = Backoff::new(RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY);
    let mut reconnecting = false;

    loop {
        if reconnecting {
            let delay = backoff.next_delay();
            tracing::info!(
                host = %host,
                port,
                delay_ms = delay.as_millis(),
                "reconnecting to OBS"
            );
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = shutdown.notified() => {
                    state.store(ConnectionState::Disconnected);
                    return;
                }
            }
        }

        state.store(if reconnecting {
            ConnectionState::Reconnecting
        } else {
            ConnectionState::Connecting
        });
        tracing::debug!(host = %host, port, "attempting OBS connection");

        let connect_config = obws::client::ConnectConfig {
            host: host.as_str(),
            port,
            password: password.as_deref(),
            event_subscriptions: Some(required_event_subscriptions()),
            broadcast_capacity: obws::client::DEFAULT_BROADCAST_CAPACITY,
            connect_timeout: obws::client::DEFAULT_CONNECT_TIMEOUT,
            dangerous: None,
        };

        match obws::Client::connect_with_config(connect_config)
            .await
            .map_err(map_obws_error)
        {
            Ok(client) => {
                let client = Arc::new(client);
                match client.general().version().await {
                    Ok(v) => {
                        let _ = obs_version.set(v.obs_studio_version.to_string());
                        let _ = obs_ws_version.set(v.obs_web_socket_version.to_string());
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to fetch OBS version");
                    }
                }

                snapshot_catalog(
                    &client,
                    &catalog_state,
                    &health_state,
                    &health_tx,
                    &item_cache,
                )
                .await;

                let events = client.events();
                inner.write().await.replace(Arc::clone(&client));

                if let Ok(mut g) = connected_at.write() {
                    *g = Some(OffsetDateTime::now_utc());
                }

                state.store(ConnectionState::Connected);
                tracing::info!(host = %host, port, "connected to OBS");
                publisher.publish(crate::events::make_connection_connected());

                // No periodic Stats event exists (OQ-OBS-1, INTEGRATIONS_NOTES.md); polled instead.
                let stats_handle = spawn_stats_poll(
                    Arc::clone(&inner),
                    Arc::clone(&health_state),
                    health_tx.clone(),
                    Arc::clone(&catalog_state),
                    Arc::clone(&item_cache),
                );

                let mut pending_resync: Option<JoinHandle<()>> = None;

                match events {
                    Ok(mut stream) => loop {
                        tokio::select! {
                            () = shutdown.notified() => {
                                if let Some(handle) = pending_resync.take() {
                                    handle.abort();
                                }
                                stats_handle.abort();
                                inner.write().await.take();
                                clear_connected_at(&connected_at);
                                state.store(ConnectionState::Disconnected);
                                tracing::info!("OBS supervisor shutting down");
                                return;
                            }
                            item = stream.next() => {
                                match item {
                                    None => {
                                        tracing::info!(host = %host, port, "OBS connection lost; reconnecting");
                                        break;
                                    }
                                    Some(ev) => {
                                        handle_obs_event(
                                            &ev,
                                            &catalog_state,
                                            &health_state,
                                            &health_tx,
                                            &item_cache,
                                            &*publisher,
                                            &last_set_scene_event_id,
                                        );
                                        // A scene collection swap replaces every scene and source
                                        // wholesale; incremental catalog updates cannot track that,
                                        // so force an immediate full resync instead of waiting for
                                        // the next reconciliation tick.
                                        if matches!(
                                            ev,
                                            obws::events::Event::CurrentSceneCollectionChanged { .. }
                                        ) {
                                            if let Some(handle) = pending_resync.take() {
                                                handle.abort();
                                            }
                                            let client = Arc::clone(&client);
                                            let catalog_state = Arc::clone(&catalog_state);
                                            let health_state = Arc::clone(&health_state);
                                            let health_tx = health_tx.clone();
                                            let item_cache = Arc::clone(&item_cache);
                                            pending_resync = Some(tokio::spawn(async move {
                                                snapshot_catalog(
                                                    &client,
                                                    &catalog_state,
                                                    &health_state,
                                                    &health_tx,
                                                    &item_cache,
                                                )
                                                .await;
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "OBS event subscription unavailable; waiting for shutdown only"
                        );
                        shutdown.notified().await;
                        if let Some(handle) = pending_resync.take() {
                            handle.abort();
                        }
                        stats_handle.abort();
                        inner.write().await.take();
                        clear_connected_at(&connected_at);
                        state.store(ConnectionState::Disconnected);
                        return;
                    }
                }

                if let Some(handle) = pending_resync.take() {
                    handle.abort();
                }
                stats_handle.abort();
                inner.write().await.take();
                clear_connected_at(&connected_at);
                let retry = auto_reconnect.load(Ordering::Relaxed);
                state.store(if retry {
                    ConnectionState::Reconnecting
                } else {
                    ConnectionState::Disconnected
                });
                publisher.publish(crate::events::make_connection_disconnected(
                    crate::payload_fields::connection::reason::CONNECTION_LOST,
                    None,
                ));
                if retry {
                    backoff.reset();
                    reconnecting = true;
                } else {
                    return;
                }
            }

            Err(ObsError::Authentication) => {
                tracing::warn!(host = %host, port, "OBS authentication rejected");
                state.store(ConnectionState::Disconnected);
                publisher.publish(crate::events::make_connection_auth_failed(
                    "authentication rejected",
                ));
                return;
            }

            Err(e) => {
                tracing::debug!(host = %host, port, error = %e, "OBS connection attempt failed");
                reconnecting = true;
            }
        }
    }
}

fn handle_obs_event(
    ev: &obws::events::Event,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    item_cache: &Mutex<HashMap<(String, String), i64>>,
    publisher: &dyn EventPublisher,
    last_set_scene_event_id: &RwLock<Option<EventId>>,
) {
    let is_scene_change = matches!(ev, obws::events::Event::CurrentProgramSceneChanged { .. });
    let is_preview_change = matches!(ev, obws::events::Event::CurrentPreviewSceneChanged { .. });

    let from_scene = if is_scene_change {
        catalog_state
            .read()
            .ok()
            .and_then(|g| g.current_scene.clone())
    } else if is_preview_change {
        catalog_state
            .read()
            .ok()
            .and_then(|g| g.current_preview_scene.clone())
    } else {
        None
    };

    if let Ok(mut catalog) = catalog_state.write() {
        crate::events::apply_catalog_update(ev, &mut catalog);
    }

    let deltas = if let Ok(mut health) = health_state.write() {
        crate::events::apply_health_update(ev, &mut health)
    } else {
        vec![]
    };
    for delta in deltas {
        let _ = health_tx.send(delta);
    }

    let cause = if is_scene_change {
        last_set_scene_event_id
            .write()
            .ok()
            .and_then(|mut g| g.take())
    } else {
        None
    };

    if let Some(bus_event) = crate::events::map_obs_event(ev, from_scene.as_deref(), cause) {
        publisher.publish(bus_event);
    }

    if let obws::events::Event::SceneItemEnableStateChanged {
        scene,
        item_id,
        enabled,
        ..
    } = ev
    {
        let source_name = item_cache
            .lock()
            .ok()
            .and_then(|guard| crate::events::resolve_source_name(&guard, &scene.name, *item_id));

        if let Some(name) = source_name {
            if let Ok(mut catalog) = catalog_state.write()
                && let Some(sources) = catalog.sources.get_mut(&scene.name)
                && let Some(info) = sources.iter_mut().find(|s| s.name == name)
            {
                info.visible = *enabled;
            }
            publisher.publish(crate::events::map_scene_item_visibility(
                &scene.name,
                &name,
                *enabled,
            ));
        }
    }

    if let obws::events::Event::SceneItemLockStateChanged {
        scene,
        item_id,
        locked,
        ..
    } = ev
    {
        let source_name = item_cache
            .lock()
            .ok()
            .and_then(|guard| crate::events::resolve_source_name(&guard, &scene.name, *item_id));

        if let Some(name) = source_name {
            if let Ok(mut catalog) = catalog_state.write()
                && let Some(sources) = catalog.sources.get_mut(&scene.name)
                && let Some(info) = sources.iter_mut().find(|s| s.name == name)
            {
                info.locked = *locked;
            }
            publisher.publish(crate::events::map_scene_item_lock(
                &scene.name,
                &name,
                *locked,
            ));
        }
    }

    if let obws::events::Event::SceneItemCreated {
        scene,
        source,
        item_id,
        ..
    } = ev
        && let Ok(mut cache) = item_cache.lock()
    {
        cache.insert((scene.name.clone(), source.name.clone()), *item_id as i64);
    }

    if let obws::events::Event::SceneItemCreated { scene, source, .. } = ev {
        let known_kind = catalog_state.read().ok().and_then(|guard| {
            guard
                .sources
                .values()
                .flatten()
                .find(|s| s.name == source.name)
                .and_then(|s| s.kind.clone())
        });
        if let Some(kind) = known_kind
            && let Ok(mut catalog) = catalog_state.write()
            && let Some(items) = catalog.sources.get_mut(&scene.name)
            && let Some(info) = items.iter_mut().find(|s| s.name == source.name)
        {
            info.kind = Some(kind);
        }
    }

    if let obws::events::Event::SceneItemRemoved { scene, source, .. } = ev
        && let Ok(mut cache) = item_cache.lock()
    {
        cache.remove(&(scene.name.clone(), source.name.clone()));
    }

    if let obws::events::Event::InputRemoved { id } = ev
        && let Ok(mut cache) = item_cache.lock()
    {
        cache.retain(|(_, name), _| name != &id.name);
    }

    if let obws::events::Event::InputNameChanged {
        old_name, new_name, ..
    } = ev
        && let Ok(mut cache) = item_cache.lock()
    {
        let renamed: Vec<(String, i64)> = cache
            .iter()
            .filter(|((_, name), _)| name == old_name)
            .map(|((scene, _), id)| (scene.clone(), *id))
            .collect();
        for (scene, id) in renamed {
            cache.remove(&(scene.clone(), old_name.clone()));
            cache.insert((scene, new_name.clone()), id);
        }
    }
}

/// Snapshots every known scene's sources, not just the active one, so `BuiltinContent::sections()`
/// has non-active scene counts available immediately after a cold connect. Also (re)populates
/// `item_cache` so live `SceneItemEnableStateChanged`/`SceneItemLockStateChanged` events can
/// resolve a source name without ever having gone through a forge-initiated visibility toggle.
async fn snapshot_catalog(
    client: &obws::Client,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    item_cache: &Mutex<HashMap<(String, String), i64>>,
) {
    use obws::requests::inputs::InputId;
    use obws::requests::scenes::SceneId;

    let scenes: Vec<String> = client
        .scenes()
        .list()
        .await
        .map(|list| list.scenes.iter().map(|s| s.id.name.clone()).collect())
        .unwrap_or_default();

    let current_scene: Option<String> = client
        .scenes()
        .current_program_scene()
        .await
        .map(|s| s.id.name.clone())
        .ok();

    let current_preview_scene: Option<String> = client
        .scenes()
        .current_preview_scene()
        .await
        .map(|s| s.id.name.clone())
        .ok();

    let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> = HashMap::new();
    let mut db_cache: HashMap<String, f32> = HashMap::new();
    let mut fresh_item_cache: HashMap<(String, String), i64> = HashMap::new();
    for scene in &scenes {
        if let Ok(items) = client.scene_items().list(SceneId::Name(scene)).await {
            let mut infos = Vec::with_capacity(items.len());
            for item in items {
                fresh_item_cache.insert((scene.clone(), item.source_name.clone()), item.id);

                let visible = client
                    .scene_items()
                    .enabled(SceneId::Name(scene), item.id)
                    .await
                    .unwrap_or(true);
                let locked = client
                    .scene_items()
                    .locked(SceneId::Name(scene), item.id)
                    .await
                    .unwrap_or(false);

                let kind = item.input_kind;
                let audio_db = if crate::catalog::is_audio_kind(kind.as_deref()) {
                    match db_cache.get(&item.source_name) {
                        Some(db) => Some(*db),
                        None => {
                            let fetched = client
                                .inputs()
                                .volume(InputId::Name(&item.source_name))
                                .await
                                .ok()
                                .map(|v| v.db);
                            if let Some(db) = fetched {
                                db_cache.insert(item.source_name.clone(), db);
                            }
                            fetched
                        }
                    }
                } else {
                    None
                };
                infos.push(SourceInfo {
                    name: item.source_name,
                    visible,
                    locked,
                    audio_db,
                    kind,
                });
            }
            sources_by_scene.insert(scene.clone(), infos);
        }
    }

    let audio_inputs: Vec<String> = client
        .inputs()
        .list(None)
        .await
        .map(|inputs| inputs.into_iter().map(|i| i.id.name.clone()).collect())
        .unwrap_or_default();

    if let Ok(mut catalog) = catalog_state.write() {
        catalog.scenes = scenes;
        catalog.audio_inputs = audio_inputs;
        catalog.sources = sources_by_scene;
        catalog.current_scene = current_scene;
        catalog.current_preview_scene = current_preview_scene;
    }
    if let Ok(mut cache) = item_cache.lock() {
        *cache = fresh_item_cache;
    }

    if let Ok(status) = client.streaming().status().await
        && let Ok(mut snapshot) = health_state.write()
    {
        for delta in crate::events::apply_stream_status_update(&status, &mut snapshot) {
            let _ = health_tx.send(delta);
        }
    }
    if let Ok(status) = client.recording().status().await
        && let Ok(mut snapshot) = health_state.write()
    {
        for delta in crate::events::apply_record_status_update(&status, &mut snapshot) {
            let _ = health_tx.send(delta);
        }
    }
}

/// Cheap safety-net reconciliation for topology drift (missed scene/source create, remove, or
/// rename events). Scoped per-scene, not per-source: it never re-fetches `enabled`/`locked`/dB,
/// which stay live via `SceneItemEnableStateChanged`/`SceneItemLockStateChanged`/
/// `InputVolumeChanged`, so cost scales with scene count rather than total source count.
async fn reconcile_catalog_topology(
    client: &obws::Client,
    catalog_state: &RwLock<ObsCatalog>,
    item_cache: &Mutex<HashMap<(String, String), i64>>,
) {
    use obws::requests::scenes::SceneId;

    let scenes: Vec<String> = client
        .scenes()
        .list()
        .await
        .map(|list| list.scenes.iter().map(|s| s.id.name.clone()).collect())
        .unwrap_or_default();

    let fetched_current_scene: Option<String> = client
        .scenes()
        .current_program_scene()
        .await
        .map(|s| s.id.name.clone())
        .ok();

    let fetched_current_preview_scene: Option<String> = client
        .scenes()
        .current_preview_scene()
        .await
        .map(|s| s.id.name.clone())
        .ok();

    let audio_inputs: Vec<String> = client
        .inputs()
        .list(None)
        .await
        .map(|inputs| inputs.into_iter().map(|i| i.id.name.clone()).collect())
        .unwrap_or_default();

    let mut fetched_topology: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
    let mut fresh_item_cache: HashMap<(String, String), i64> = HashMap::new();
    for scene in &scenes {
        if let Ok(items) = client.scene_items().list(SceneId::Name(scene)).await {
            let mut entries = Vec::with_capacity(items.len());
            for item in items {
                fresh_item_cache.insert((scene.clone(), item.source_name.clone()), item.id);
                entries.push((item.source_name, item.input_kind));
            }
            fetched_topology.insert(scene.clone(), entries);
        }
    }

    if let Ok(mut catalog) = catalog_state.write() {
        let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> =
            HashMap::with_capacity(fetched_topology.len());
        for (scene, entries) in fetched_topology {
            let live = catalog.sources.get(&scene);
            let mut infos = Vec::with_capacity(entries.len());
            for (name, kind) in entries {
                let live_info = live.and_then(|known| known.iter().find(|s| s.name == name));
                infos.push(SourceInfo {
                    visible: live_info.map(|s| s.visible).unwrap_or(true),
                    locked: live_info.map(|s| s.locked).unwrap_or(false),
                    audio_db: live_info.and_then(|s| s.audio_db),
                    kind: kind.or_else(|| live_info.and_then(|s| s.kind.clone())),
                    name,
                });
            }
            sources_by_scene.insert(scene, infos);
        }

        catalog.scenes = scenes;
        catalog.audio_inputs = audio_inputs;
        catalog.sources = sources_by_scene;
        catalog.current_scene = catalog.current_scene.take().or(fetched_current_scene);
        catalog.current_preview_scene = catalog
            .current_preview_scene
            .take()
            .or(fetched_current_preview_scene);
    }
    if let Ok(mut cache) = item_cache.lock() {
        *cache = fresh_item_cache;
    }
}

const STATS_POLL_INTERVAL: Duration = Duration::from_secs(2);
const CATALOG_RECONCILE_EVERY_NTH_TICK: u32 = 3;

/// The returned handle MUST be `.abort()`-ed on every connection-loss/shutdown exit path;
/// dropping a `JoinHandle` does not cancel the underlying task.
fn spawn_stats_poll(
    inner: Arc<tokio::sync::RwLock<Option<Arc<obws::Client>>>>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    catalog_state: Arc<RwLock<ObsCatalog>>,
    item_cache: Arc<Mutex<HashMap<(String, String), i64>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(STATS_POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut tick: u32 = 0;
        loop {
            ticker.tick().await;
            tick = tick.wrapping_add(1);

            let client = {
                let guard = inner.read().await;
                let Some(client) = guard.as_ref() else {
                    continue;
                };
                Arc::clone(client)
            };
            let general = client.general();
            let streaming = client.streaming();
            let recording = client.recording();
            let (stats, stream_status, record_status) =
                tokio::join!(general.stats(), streaming.status(), recording.status());

            let mut deltas = Vec::new();
            if let Ok(stats) = stats
                && let Ok(mut snapshot) = health_state.write()
            {
                deltas.extend(crate::events::apply_stats_update(&stats, &mut snapshot));
            }
            if let Ok(status) = stream_status
                && let Ok(mut snapshot) = health_state.write()
            {
                deltas.extend(crate::events::apply_stream_status_update(
                    &status,
                    &mut snapshot,
                ));
            }
            if let Ok(status) = record_status
                && let Ok(mut snapshot) = health_state.write()
            {
                deltas.extend(crate::events::apply_record_status_update(
                    &status,
                    &mut snapshot,
                ));
            }
            for delta in deltas {
                let _ = health_tx.send(delta);
            }

            if tick.is_multiple_of(CATALOG_RECONCILE_EVERY_NTH_TICK) {
                reconcile_catalog_topology(&client, &catalog_state, &item_cache).await;
            }
        }
    })
}

pub fn parse_endpoint(endpoint: &str) -> Result<(String, u16), ObsError> {
    let without_scheme = endpoint
        .strip_prefix("ws://")
        .or_else(|| endpoint.strip_prefix("wss://"))
        .unwrap_or(endpoint);

    match without_scheme.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str
                .parse::<u16>()
                .map_err(|_| ObsError::Connect(format!("invalid port in endpoint '{endpoint}'")))?;
            Ok((host.to_owned(), port))
        }
        None => Ok((without_scheme.to_owned(), 4455)),
    }
}

/// Excludes every high-volume opt-in category (volume meters, input active/show state,
/// scene-item transform) so the bus is never flooded by a continuous stream.
fn required_event_subscriptions() -> obws::requests::EventSubscription {
    use obws::requests::EventSubscription as Sub;
    Sub::SCENES
        | Sub::CONFIG
        | Sub::OUTPUTS
        | Sub::SCENE_ITEMS
        | Sub::TRANSITIONS
        | Sub::UI
        | Sub::INPUTS
        | Sub::FILTERS
}

fn clear_connected_at(connected_at: &RwLock<Option<OffsetDateTime>>) {
    if let Ok(mut g) = connected_at.write() {
        *g = None;
    }
}

fn describe_error_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![e.to_string()];
    let mut current = e.source();
    while let Some(source) = current {
        parts.push(source.to_string());
        current = source.source();
    }
    parts.join(": ")
}

pub(crate) fn map_obws_error(e: obws::error::Error) -> ObsError {
    match &e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected => ObsError::Disconnected,
        obws::error::Error::Handshake(obws::client::HandshakeError::NoIdentified) => {
            ObsError::Authentication
        }
        obws::error::Error::Handshake(obws::client::HandshakeError::ConnectionClosed(Some(
            details,
        ))) if u16::from(details.code)
            == u16::from(obws::responses::WebSocketCloseCode::AuthenticationFailed) =>
        {
            ObsError::Authentication
        }
        _ => ObsError::Connect(describe_error_chain(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_drops_the_scheme_and_falls_back_to_the_obs_websocket_port() {
        for (endpoint, expected_host, expected_port) in [
            ("ws://localhost:4455", "localhost", 4455),
            ("wss://obs.example.com:4456", "obs.example.com", 4456),
            ("192.168.1.10:4455", "192.168.1.10", 4455),
            ("localhost", "localhost", 4455),
            ("ws://localhost", "localhost", 4455),
            ("localhost:65535", "localhost", 65535),
        ] {
            let (host, port) = parse_endpoint(endpoint).unwrap();
            assert_eq!((host.as_str(), port), (expected_host, expected_port));
        }
    }

    #[test]
    fn parse_endpoint_rejects_a_port_that_is_not_a_u16() {
        for endpoint in [
            "localhost:notaport",
            "localhost:",
            "localhost:65536",
            "localhost:-1",
            "localhost:44 55",
        ] {
            assert!(
                parse_endpoint(endpoint).is_err(),
                "expected {endpoint} to be rejected"
            );
        }
    }

    // Why: an OBS restart mid-stream must heal itself, so reconnection stays on until the user
    // turns it off from the connection settings; a fresh client must never start out gated off.
    #[test]
    fn auto_reconnect_starts_enabled_and_follows_the_user_toggle() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        assert!(client.auto_reconnect_enabled());
        client.set_auto_reconnect(false);
        assert!(!client.auto_reconnect_enabled());
        client.set_auto_reconnect(true);
        assert!(client.auto_reconnect_enabled());
    }

    #[tokio::test]
    async fn refresh_token_is_unsupported() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let outcome = forge_platform_core::BuiltinControl::refresh_token(&client).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::Unsupported)
        );
    }

    #[derive(Default)]
    struct CapturingPublisher(Mutex<Vec<forge_events::Event>>);

    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: forge_events::Event) {
            self.0.lock().unwrap_or_else(|p| p.into_inner()).push(event);
        }
    }

    impl CapturingPublisher {
        fn kinds(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .map(|e| e.kind.clone())
                .collect()
        }

        fn last_payload(&self, field: &str) -> Option<serde_json::Value> {
            self.0
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .last()
                .map(|e| e.payload[field].clone())
        }
    }

    struct EventHarness {
        catalog: RwLock<ObsCatalog>,
        health: RwLock<HealthSnapshot>,
        health_tx: broadcast::Sender<HealthDelta>,
        item_cache: Mutex<HashMap<(String, String), i64>>,
        publisher: CapturingPublisher,
        last_set_scene_event_id: RwLock<Option<EventId>>,
    }

    impl EventHarness {
        fn new() -> Self {
            let (health_tx, _) = broadcast::channel(16);
            Self {
                catalog: RwLock::new(ObsCatalog::default()),
                health: RwLock::new(HealthSnapshot::default()),
                health_tx,
                item_cache: Mutex::new(HashMap::new()),
                publisher: CapturingPublisher::default(),
                last_set_scene_event_id: RwLock::new(None),
            }
        }

        fn with_source(self, scene: &str, source: &str, item_id: i64) -> Self {
            if let Ok(mut catalog) = self.catalog.write() {
                catalog
                    .sources
                    .entry(scene.to_owned())
                    .or_default()
                    .push(SourceInfo {
                        name: source.to_owned(),
                        visible: true,
                        locked: false,
                        audio_db: None,
                        kind: None,
                    });
            }
            if let Ok(mut cache) = self.item_cache.lock() {
                cache.insert((scene.to_owned(), source.to_owned()), item_id);
            }
            self
        }

        fn feed(&self, ev: &obws::events::Event) {
            handle_obs_event(
                ev,
                &self.catalog,
                &self.health,
                &self.health_tx,
                &self.item_cache,
                &self.publisher,
                &self.last_set_scene_event_id,
            );
        }

        fn source_flags(&self, scene: &str, source: &str) -> Option<(bool, bool)> {
            let catalog = self.catalog.read().ok()?;
            catalog
                .sources
                .get(scene)?
                .iter()
                .find(|s| s.name == source)
                .map(|s| (s.visible, s.locked))
        }
    }

    fn scene_id(name: &str) -> obws::responses::scenes::SceneId {
        obws::responses::scenes::SceneId {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn source_id(name: &str) -> obws::responses::sources::SourceId {
        obws::responses::sources::SourceId {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn enable_state_changed(scene: &str, item_id: u64, enabled: bool) -> obws::events::Event {
        obws::events::Event::SceneItemEnableStateChanged {
            scene: scene_id(scene),
            item_id,
            enabled,
        }
    }

    // Why: a source hidden from inside OBS reaches forge as a numeric item id only. Before the
    // id cache was seeded from the catalog snapshot, only items forge had toggled itself could be
    // resolved, so hiding a source in OBS left the panel row and the bus event missing.
    #[test]
    fn hiding_a_scene_item_inside_obs_updates_the_catalog_row_and_publishes_the_change() {
        let h = EventHarness::new().with_source("Gameplay", "Webcam", 42);

        h.feed(&enable_state_changed("Gameplay", 42, false));

        assert_eq!(h.source_flags("Gameplay", "Webcam"), Some((false, false)));
        assert_eq!(h.publisher.kinds(), vec!["obs.source.visibility_changed"]);
        assert_eq!(
            h.publisher.last_payload("source_name"),
            Some(serde_json::json!("Webcam")),
        );
    }

    #[test]
    fn locking_a_scene_item_inside_obs_updates_the_catalog_row_and_publishes_the_change() {
        let h = EventHarness::new().with_source("Gameplay", "Webcam", 42);

        h.feed(&obws::events::Event::SceneItemLockStateChanged {
            scene: scene_id("Gameplay"),
            item_id: 42,
            locked: true,
        });

        assert_eq!(h.source_flags("Gameplay", "Webcam"), Some((true, true)));
        assert_eq!(h.publisher.kinds(), vec!["obs.source.lock_changed"]);
    }

    #[test]
    fn a_scene_item_id_that_is_not_cached_changes_nothing_and_publishes_nothing() {
        let h = EventHarness::new().with_source("Gameplay", "Webcam", 42);

        h.feed(&enable_state_changed("Gameplay", 99, false));

        assert_eq!(h.source_flags("Gameplay", "Webcam"), Some((true, false)));
        assert!(h.publisher.kinds().is_empty());
    }

    #[test]
    fn the_id_cache_follows_a_scene_item_from_creation_to_removal() {
        let h = EventHarness::new();

        h.feed(&obws::events::Event::SceneItemCreated {
            scene: scene_id("Gameplay"),
            source: source_id("Webcam"),
            item_id: 42,
            index: 0,
        });
        h.feed(&enable_state_changed("Gameplay", 42, false));
        assert_eq!(
            h.publisher.kinds().last().map(String::as_str),
            Some("obs.source.visibility_changed"),
            "a freshly created item id could not be resolved",
        );

        h.feed(&obws::events::Event::SceneItemRemoved {
            scene: scene_id("Gameplay"),
            source: source_id("Webcam"),
            item_id: 42,
        });
        let before = h.publisher.kinds().len();
        h.feed(&enable_state_changed("Gameplay", 42, true));
        assert_eq!(
            h.publisher.kinds().len(),
            before,
            "a removed item id still resolved to a source",
        );
    }

    #[test]
    fn a_renamed_input_keeps_its_cached_item_id_under_the_new_source_name() {
        let h = EventHarness::new().with_source("Gameplay", "Mic", 42);

        h.feed(&obws::events::Event::InputNameChanged {
            uuid: Default::default(),
            old_name: "Mic".to_owned(),
            new_name: "Studio Mic".to_owned(),
        });
        h.feed(&enable_state_changed("Gameplay", 42, false));

        assert_eq!(
            h.publisher.kinds().last().map(String::as_str),
            Some("obs.source.visibility_changed"),
        );
        assert_eq!(
            h.publisher.last_payload("source_name"),
            Some(serde_json::json!("Studio Mic")),
        );
    }

    #[test]
    fn a_removed_input_evicts_its_cached_item_ids_from_every_scene() {
        let h = EventHarness::new()
            .with_source("Gameplay", "Mic", 42)
            .with_source("BRB", "Mic", 43);

        h.feed(&obws::events::Event::InputRemoved {
            id: obws::responses::inputs::InputId {
                name: "Mic".to_owned(),
                ..Default::default()
            },
        });
        let before = h.publisher.kinds().len();
        h.feed(&enable_state_changed("Gameplay", 42, false));
        h.feed(&enable_state_changed("BRB", 43, false));

        assert_eq!(
            h.publisher.kinds().len(),
            before,
            "a removed input still resolved a cached item id: {:?}",
            h.publisher.kinds(),
        );
    }
}
