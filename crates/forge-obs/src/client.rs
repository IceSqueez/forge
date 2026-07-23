use std::collections::HashMap;
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
    ConnectionState, ControlFailure, ControlOutcome, HeaderAction, HealthDelta, HealthValue,
};
use forge_types::EventId;

use crate::catalog::ObsCatalog;
use crate::error::ObsError;
use crate::health::{HealthSnapshot, make_health_channel};
use crate::source::SourceInfo;

pub struct ObsClient {
    pub(crate) inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
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
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) catalog_state: Arc<RwLock<ObsCatalog>>,
    reconnect_host: String,
    reconnect_port: u16,
    // Never logged or surfaced.
    reconnect_password: Arc<Option<String>>,
    reconnect_publisher: Arc<dyn EventPublisher>,
}

impl ObsClient {
    pub async fn connect(
        endpoint: &str,
        password: Option<&str>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Self, ObsError> {
        let (host, port) = parse_endpoint(endpoint)?;

        let inner = Arc::new(tokio::sync::RwLock::new(None::<obws::Client>));
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connecting));
        let notify = Arc::new(Notify::new());
        let shutdown = Arc::new(tokio::sync::Mutex::new(Arc::clone(&notify)));
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let obs_version = Arc::new(OnceLock::new());
        let item_cache = Arc::new(Mutex::new(HashMap::<(String, String), i64>::new()));

        let (health_tx, health_state) = make_health_channel();
        let catalog_state = Arc::new(RwLock::new(ObsCatalog::default()));
        let last_set_scene_event_id = Arc::new(RwLock::new(None::<EventId>));

        let stored_password = password.map(str::to_owned);

        let ctx = SupervisorContext {
            inner: Arc::clone(&inner),
            state: Arc::clone(&state),
            shutdown: Arc::clone(&notify),
            connected_at: Arc::clone(&connected_at),
            obs_version: Arc::clone(&obs_version),
            catalog_state: Arc::clone(&catalog_state),
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            publisher: Arc::clone(&publisher),
            item_cache: Arc::clone(&item_cache),
            last_set_scene_event_id: Arc::clone(&last_set_scene_event_id),
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
            health_state,
            health_tx,
            catalog_state,
            scene_item_id_cache: item_cache,
            last_set_scene_event_id,
            reconnect_host: host,
            reconnect_port: port,
            reconnect_password: Arc::new(stored_password),
            reconnect_publisher: publisher,
        })
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state.load()
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
            health_state,
            health_tx,
            catalog_state: Arc::new(RwLock::new(ObsCatalog::default())),
            scene_item_id_cache: Arc::new(Mutex::new(HashMap::new())),
            last_set_scene_event_id: Arc::new(RwLock::new(None)),
            reconnect_host: host,
            reconnect_port: port,
            reconnect_password: Arc::new(None),
            reconnect_publisher: Arc::new(crate::runners::test_support::NoopPublisher),
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
        vec![
            HeaderAction::Reconnect,
            HeaderAction::Disconnect,
            HeaderAction::Settings,
        ]
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
        if let Ok(mut g) = self.connected_at.write() {
            *g = None;
        }

        let ctx = SupervisorContext {
            inner: Arc::clone(&self.inner),
            state: Arc::clone(&self.state),
            shutdown: new_notify,
            connected_at: Arc::clone(&self.connected_at),
            obs_version: Arc::clone(&self.obs_version),
            catalog_state: Arc::clone(&self.catalog_state),
            health_state: Arc::clone(&self.health_state),
            health_tx: self.health_tx.clone(),
            publisher: Arc::clone(&self.reconnect_publisher),
            item_cache: Arc::clone(&self.scene_item_id_cache),
            last_set_scene_event_id: Arc::clone(&self.last_set_scene_event_id),
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
    inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
    state: Arc<AtomicConnectionState>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    obs_version: Arc<OnceLock<String>>,
    catalog_state: Arc<RwLock<ObsCatalog>>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    publisher: Arc<dyn EventPublisher>,
    item_cache: Arc<Mutex<HashMap<(String, String), i64>>>,
    last_set_scene_event_id: Arc<RwLock<Option<EventId>>>,
}

async fn run_supervisor(host: String, port: u16, password: Option<String>, ctx: SupervisorContext) {
    let SupervisorContext {
        inner,
        state,
        shutdown,
        connected_at,
        obs_version,
        catalog_state,
        health_state,
        health_tx,
        publisher,
        item_cache,
        last_set_scene_event_id,
    } = ctx;
    let mut backoff = Backoff::default();
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
                match client.general().version().await {
                    Ok(v) => {
                        let _ = obs_version.set(v.obs_studio_version.to_string());
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to fetch OBS version");
                    }
                }

                snapshot_catalog(&client, &catalog_state, &health_state, &health_tx).await;

                let events = client.events();
                inner.write().await.replace(client);

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
                );

                match events {
                    Ok(mut stream) => loop {
                        tokio::select! {
                            () = shutdown.notified() => {
                                stats_handle.abort();
                                inner.write().await.take();
                                state.store(ConnectionState::Disconnected);
                                tracing::info!("OBS supervisor shutting down");
                                return;
                            }
                            item = stream.next() => {
                                match item {
                                    None => {
                                        tracing::info!(host = %host, port, "OBS connection lost; reconnecting");
                                        publisher.publish(crate::events::make_connection_disconnected(
                                            crate::payload_fields::connection::reason::CONNECTION_LOST,
                                            None,
                                        ));
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
                        stats_handle.abort();
                        inner.write().await.take();
                        state.store(ConnectionState::Disconnected);
                        return;
                    }
                }

                stats_handle.abort();
                inner.write().await.take();
                backoff.reset();
                reconnecting = true;
            }

            Err(ObsError::Authentication) => {
                tracing::warn!(host = %host, port, "OBS authentication rejected");
                publisher.publish(crate::events::make_connection_auth_failed(
                    "authentication rejected",
                ));
                state.store(ConnectionState::Disconnected);
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
            publisher.publish(crate::events::map_scene_item_lock(
                &scene.name,
                &name,
                *locked,
            ));
        }
    }
}

/// Snapshots every known scene's sources, not just the active one, so `BuiltinContent::sections()`
/// has non-active scene counts available immediately after a cold connect.
async fn snapshot_catalog(
    client: &obws::Client,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
) {
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

    let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> = HashMap::new();
    for scene in &scenes {
        if let Ok(items) = client.scene_items().list(SceneId::Name(scene)).await {
            let infos = items
                .into_iter()
                .map(|i| SourceInfo {
                    name: i.source_name,
                    visible: true,
                    locked: false,
                    audio_db: None,
                })
                .collect();
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
    }

    if let Ok(status) = client.streaming().status().await {
        seed_health_status(health_state, health_tx, 0, status.active, |a| {
            crate::events::make_stream_health_value(a)
        });
    }
    if let Ok(status) = client.recording().status().await {
        seed_health_status(health_state, health_tx, 1, status.active, |a| {
            crate::events::make_record_health_value(a)
        });
    }
}

/// Only broadcasts a delta when the value actually differs from the persisted snapshot, so a
/// reconnect never emits a spurious duplicate delta.
fn seed_health_status(
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    index: u8,
    active: bool,
    make_value: impl Fn(bool) -> HealthValue,
) {
    let changed = match health_state.write() {
        Ok(mut snap) => {
            let field = if index == 0 {
                &mut snap.stream_active
            } else {
                &mut snap.record_active
            };
            let changed = *field != active;
            *field = active;
            changed
        }
        Err(_) => false,
    };
    if changed {
        let _ = health_tx.send(HealthDelta {
            index,
            new_value: make_value(active),
        });
    }
}

const STATS_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The returned handle MUST be `.abort()`-ed on every connection-loss/shutdown exit path;
/// dropping a `JoinHandle` does not cancel the underlying task.
fn spawn_stats_poll(
    inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(STATS_POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;

            let stats = {
                let guard = inner.read().await;
                let Some(client) = guard.as_ref() else {
                    continue;
                };
                client.general().stats().await
            };
            let Ok(stats) = stats else {
                continue;
            };

            let deltas = match health_state.write() {
                Ok(mut snapshot) => crate::events::apply_stats_update(&stats, &mut snapshot),
                Err(_) => continue,
            };
            for delta in deltas {
                let _ = health_tx.send(delta);
            }
        }
    })
}

fn parse_endpoint(endpoint: &str) -> Result<(String, u16), ObsError> {
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

fn map_obws_error(e: obws::error::Error) -> ObsError {
    match &e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected => ObsError::Disconnected,
        obws::error::Error::Handshake(obws::client::HandshakeError::NoIdentified) => {
            ObsError::Authentication
        }
        _ => ObsError::Connect(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_with_scheme_and_port() {
        let (host, port) = parse_endpoint("ws://localhost:4455").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 4455);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_without_scheme() {
        let (host, port) = parse_endpoint("192.168.1.10:4455").unwrap();
        assert_eq!(host, "192.168.1.10");
        assert_eq!(port, 4455);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_default_port() {
        let (host, port) = parse_endpoint("localhost").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 4455);
    }

    #[test]
    fn parse_endpoint_invalid_port_errors() {
        assert!(parse_endpoint("localhost:notaport").is_err());
    }

    #[test]
    fn client_coerces_to_dyn_builtin_control() {
        fn accepts(_: Arc<dyn forge_platform_core::BuiltinControl>) {}
        accepts(Arc::new(ObsClient::new_for_test(
            "localhost:4455".to_owned(),
        )));
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
}
