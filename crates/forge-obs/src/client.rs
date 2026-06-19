use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use rand::RngExt;
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use forge_events::EventPublisher;
use forge_platform_core::{
    BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction, HealthDelta,
};
use forge_types::EventId;

use crate::catalog::ObsCatalog;
use crate::error::ObsError;
use crate::health::{HealthSnapshot, make_health_channel};
use crate::source::SourceInfo;

const STATE_DISCONNECTED: u8 = 0;
const STATE_CONNECTING: u8 = 1;
const STATE_CONNECTED: u8 = 2;
const STATE_RECONNECTING: u8 = 3;

pub struct ObsClient {
    pub(crate) inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
    pub(crate) scene_item_id_cache: Arc<Mutex<HashMap<(String, String), i64>>>,
    pub(crate) last_set_scene_event_id: Arc<RwLock<Option<EventId>>>,
    endpoint: String,
    state: Arc<AtomicU8>,
    shutdown: Arc<Notify>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    obs_id: BuiltinId,
    obs_version: Arc<OnceLock<String>>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) catalog_state: Arc<RwLock<ObsCatalog>>,
}

impl ObsClient {
    pub async fn connect(
        endpoint: &str,
        password: Option<&str>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Self, ObsError> {
        let (host, port) = parse_endpoint(endpoint)?;

        let inner = Arc::new(tokio::sync::RwLock::new(None::<obws::Client>));
        let state = Arc::new(AtomicU8::new(STATE_CONNECTING));
        let shutdown = Arc::new(Notify::new());
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let obs_version = Arc::new(OnceLock::new());
        let item_cache = Arc::new(Mutex::new(HashMap::<(String, String), i64>::new()));

        let (health_tx, health_state) = make_health_channel();
        let catalog_state = Arc::new(RwLock::new(ObsCatalog::default()));
        let last_set_scene_event_id = Arc::new(RwLock::new(None::<EventId>));

        let ctx = SupervisorContext {
            inner: Arc::clone(&inner),
            state: Arc::clone(&state),
            shutdown: Arc::clone(&shutdown),
            connected_at: Arc::clone(&connected_at),
            obs_version: Arc::clone(&obs_version),
            catalog_state: Arc::clone(&catalog_state),
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            publisher,
            item_cache: Arc::clone(&item_cache),
            last_set_scene_event_id: Arc::clone(&last_set_scene_event_id),
        };
        let handle = tokio::spawn(run_supervisor(host, port, password.map(str::to_owned), ctx));

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
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn connected_at(&self) -> Option<OffsetDateTime> {
        self.connected_at.read().ok().and_then(|g| *g)
    }

    pub fn connection_state(&self) -> ConnectionState {
        match self.state.load(Ordering::Acquire) {
            STATE_CONNECTED => ConnectionState::Connected,
            STATE_CONNECTING => ConnectionState::Connecting,
            STATE_RECONNECTING => ConnectionState::Reconnecting,
            _ => ConnectionState::Disconnected,
        }
    }

    pub fn health_snapshot(&self) -> HealthSnapshot {
        self.health_state
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    #[cfg(test)]
    pub fn new_for_test(endpoint: String) -> Self {
        let (health_tx, health_state) = make_health_channel();
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(None)),
            endpoint,
            state: Arc::new(AtomicU8::new(STATE_DISCONNECTED)),
            shutdown: Arc::new(Notify::new()),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            obs_id: BuiltinId::new("obs"),
            obs_version: Arc::new(OnceLock::new()),
            health_state,
            health_tx,
            catalog_state: Arc::new(RwLock::new(ObsCatalog::default())),
            scene_item_id_cache: Arc::new(Mutex::new(HashMap::new())),
            last_set_scene_event_id: Arc::new(RwLock::new(None)),
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

pub(crate) fn compute_backoff(attempt: u32) -> Duration {
    let base_secs = (1u64 << attempt.min(6)).min(60);
    let max_jitter_ms = base_secs * 100;
    let jitter_ms = rand::rng().random_range(0..=max_jitter_ms);
    Duration::from_millis(base_secs * 1000 + jitter_ms)
}

struct SupervisorContext {
    inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
    state: Arc<AtomicU8>,
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
    let mut attempt: u32 = 0;

    loop {
        if attempt > 0 {
            let delay = compute_backoff(attempt - 1);
            tracing::info!(
                host = %host,
                port,
                attempt,
                delay_ms = delay.as_millis(),
                "reconnecting to OBS"
            );
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = shutdown.notified() => {
                    state.store(STATE_DISCONNECTED, Ordering::Release);
                    return;
                }
            }
        }

        let conn_state = if attempt == 0 {
            STATE_CONNECTING
        } else {
            STATE_RECONNECTING
        };
        state.store(conn_state, Ordering::Release);
        tracing::debug!(host = %host, port, attempt, "attempting OBS connection");

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

                snapshot_catalog(&client, &catalog_state).await;

                let events = client.events();
                inner.write().await.replace(client);

                if let Ok(mut g) = connected_at.write() {
                    *g = Some(OffsetDateTime::now_utc());
                }

                state.store(STATE_CONNECTED, Ordering::Release);
                tracing::info!(host = %host, port, "connected to OBS");

                match events {
                    Ok(mut stream) => loop {
                        tokio::select! {
                            () = shutdown.notified() => {
                                inner.write().await.take();
                                state.store(STATE_DISCONNECTED, Ordering::Release);
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
                        inner.write().await.take();
                        state.store(STATE_DISCONNECTED, Ordering::Release);
                        return;
                    }
                }

                inner.write().await.take();
                attempt = 1;
            }

            Err(ObsError::Authentication) => {
                tracing::warn!(host = %host, port, "OBS authentication rejected");
                state.store(STATE_DISCONNECTED, Ordering::Release);
                return;
            }

            Err(e) => {
                tracing::debug!(host = %host, port, attempt, error = %e, "OBS connection attempt failed");
                attempt = attempt.saturating_add(1);
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

async fn snapshot_catalog(client: &obws::Client, catalog_state: &RwLock<ObsCatalog>) {
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

    let sources: Option<(String, Vec<SourceInfo>)> = if let Some(scene) = current_scene.as_deref() {
        client
            .scene_items()
            .list(SceneId::Name(scene))
            .await
            .map(|items| {
                let infos = items
                    .into_iter()
                    .map(|i| SourceInfo {
                        name: i.source_name,
                        visible: true,
                        locked: false,
                        audio_db: None,
                    })
                    .collect();
                (scene.to_owned(), infos)
            })
            .ok()
    } else {
        None
    };

    let audio_inputs: Vec<String> = client
        .inputs()
        .list(None)
        .await
        .map(|inputs| inputs.into_iter().map(|i| i.id.name.clone()).collect())
        .unwrap_or_default();

    if let Ok(mut catalog) = catalog_state.write() {
        catalog.scenes = scenes;
        catalog.audio_inputs = audio_inputs;
        if let Some(scene) = current_scene {
            if let Some((scene_key, infos)) = sources {
                catalog.sources.insert(scene_key, infos);
            }
            catalog.current_scene = Some(scene);
        }
    }
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

/// Event categories the registered OBS trigger / health descriptors need. The union excludes
/// every high-volume opt-in category (volume meters, input active/show state, scene-item
/// transform) so the bus is never flooded by a continuous stream. The `EventSubscription`
/// type never crosses the crate boundary.
fn required_event_subscriptions() -> obws::requests::EventSubscription {
    use obws::requests::EventSubscription as Sub;
    // SCENES: program / preview / scene-list. CONFIG: scene-collection lifecycle.
    // OUTPUTS: stream + record state (health metrics). SCENE_ITEMS: source visibility.
    // TRANSITIONS: SceneTransitionStarted/Ended/VideoEnded. UI: StudioModeStateChanged.
    // INPUTS: mute / volume / balance / sync-offset. INPUT_VOLUME_METERS deliberately excluded.
    Sub::SCENES
        | Sub::CONFIG
        | Sub::OUTPUTS
        | Sub::SCENE_ITEMS
        | Sub::TRANSITIONS
        | Sub::UI
        | Sub::INPUTS
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
    fn compute_backoff_caps_at_60s_for_attempt_six() {
        let d = compute_backoff(6);
        assert!(d.as_secs() >= 60);
    }

    #[test]
    fn compute_backoff_caps_at_60s_for_attempt_seven() {
        let d = compute_backoff(7);
        assert!(d.as_secs() >= 60);
    }

    #[test]
    fn compute_backoff_first_attempt_under_two_seconds() {
        let d = compute_backoff(0);
        assert!(d.as_millis() < 2_000);
    }

    #[test]
    fn compute_backoff_attempt_five_under_60s() {
        let d = compute_backoff(5);
        assert!(d.as_secs() < 60);
    }

    #[test]
    fn compute_backoff_base_doubles_each_attempt_before_cap() {
        for attempt in 0u32..5 {
            let base_secs_this = 1u64 << attempt;
            let base_secs_next = 1u64 << (attempt + 1);
            assert_eq!(
                base_secs_next,
                base_secs_this * 2,
                "base must double from attempt {attempt} to {}",
                attempt + 1
            );
        }
    }

    #[test]
    fn compute_backoff_total_millis_at_least_base_secs() {
        for attempt in 0u32..=7 {
            let d = compute_backoff(attempt);
            let base_secs = (1u64 << attempt.min(6)).min(60);
            assert!(
                d.as_millis() >= (base_secs * 1000) as u128,
                "backoff for attempt {attempt} must be >= {base_secs}s but was {}ms",
                d.as_millis()
            );
        }
    }
}
