use crate::vocabulary::labels;
use forge_types::VerbatimText;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast, watch};
use tokio::task::JoinHandle;

use forge_events::{Event, EventPublisher};
use forge_net::Backoff;
use forge_platform_core::{
    AtomicConnectionState, BuiltinControl, BuiltinStatus, CapabilityFlags,
    CollectionRevisionSignal, ConnectionState, ConnectionTransition, ControlFailure,
    ControlOutcome, HeaderAction, HealthDelta, HeroBadge, HeroBadgeTone,
    connection_state_changed_event,
};
use forge_types::{IntegrationId, MessageRef};

use crate::catalog::{ObsCatalog, read_lock, write_lock};
use crate::connect_failure::connect_failure_message;
use crate::error::ObsError;
use crate::health::{HealthSnapshot, make_health_channel};
use crate::item_ids::SceneItemIds;
use crate::session::{LiveSession, with_deadline};
use crate::source::SourceInfo;
use crate::stream_output::StreamOutputFeed;
use crate::vocabulary::messages;

const OBS_PLATFORM_ID: &str = crate::vocabulary::INTEGRATION_ID;

fn connection_state_transition(
    previous: ConnectionState,
    next: ConnectionState,
    transition: ConnectionTransition,
) -> Option<Event> {
    (previous != next).then(|| connection_state_changed_event(OBS_PLATFORM_ID, next, transition))
}

fn settle_connection_state(
    state: &AtomicConnectionState,
    publisher: &dyn EventPublisher,
    stream_output: &StreamOutputFeed,
    next: ConnectionState,
    transition: ConnectionTransition,
) {
    let previous = state.load();
    state.store(next);
    stream_output.refresh();
    if let Some(event) = connection_state_transition(previous, next, transition) {
        publisher.publish(event);
    }
}

struct AbortOnDrop(JoinHandle<()>);

impl AbortOnDrop {
    fn abort(&self) {
        self.0.abort();
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type SessionSlot = Arc<tokio::sync::RwLock<Option<LiveSession>>>;

pub struct ObsClient {
    pub(crate) inner: SessionSlot,
    pub(crate) scene_item_id_cache: Arc<Mutex<SceneItemIds>>,
    endpoint: String,
    state: Arc<AtomicConnectionState>,
    shutdown: Arc<tokio::sync::Mutex<Arc<Notify>>>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    obs_id: IntegrationId,
    obs_version: Arc<OnceLock<String>>,
    obs_ws_version: Arc<RwLock<Option<String>>>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) catalog_state: Arc<RwLock<ObsCatalog>>,
    pub(crate) option_revisions: Arc<CollectionRevisionSignal>,
    reconnect_host: String,
    reconnect_port: u16,
    reconnect_password: Arc<Option<String>>,
    reconnect_publisher: Arc<dyn EventPublisher>,
    auto_reconnect: Arc<watch::Sender<bool>>,
    last_failure: Arc<RwLock<Option<MessageRef>>>,
    resync_nudge: Arc<Notify>,
    pub(crate) stream_output: StreamOutputFeed,
}

impl ObsClient {
    pub async fn connect(
        endpoint: &str,
        password: Option<&str>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Self, ObsError> {
        let (host, port) = parse_endpoint(endpoint)?;

        let inner: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connecting));
        let notify = Arc::new(Notify::new());
        let shutdown = Arc::new(tokio::sync::Mutex::new(Arc::clone(&notify)));
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let obs_version = Arc::new(OnceLock::new());
        let obs_ws_version = Arc::new(RwLock::new(None::<String>));
        let item_cache = Arc::new(Mutex::new(SceneItemIds::default()));

        let (health_tx, health_state) = make_health_channel();
        let catalog_state = Arc::new(RwLock::new(ObsCatalog::default()));
        let option_revisions = Arc::new(CollectionRevisionSignal::new());

        let stored_password = password.map(str::to_owned);
        let auto_reconnect = Arc::new(watch::Sender::new(true));
        let last_failure = Arc::new(RwLock::new(None::<MessageRef>));
        let resync_nudge = Arc::new(Notify::new());
        let stream_output = StreamOutputFeed::new(Arc::clone(&state), Arc::clone(&health_state));

        let ctx = SupervisorContext {
            inner: Arc::clone(&inner),
            state: Arc::clone(&state),
            shutdown: Arc::clone(&notify),
            connected_at: Arc::clone(&connected_at),
            obs_version: Arc::clone(&obs_version),
            obs_ws_version: Arc::clone(&obs_ws_version),
            catalog_state: Arc::clone(&catalog_state),
            option_revisions: Arc::clone(&option_revisions),
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            publisher: Arc::clone(&publisher),
            item_cache: Arc::clone(&item_cache),
            auto_reconnect: auto_reconnect.subscribe(),
            last_failure: Arc::clone(&last_failure),
            resync_nudge: Arc::clone(&resync_nudge),
            stream_output: stream_output.clone(),
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
            obs_id: crate::OBS_INTEGRATION.id,
            obs_version,
            obs_ws_version,
            health_state,
            health_tx,
            catalog_state,
            option_revisions,
            scene_item_id_cache: item_cache,
            reconnect_host: host,
            reconnect_port: port,
            reconnect_password: Arc::new(stored_password),
            reconnect_publisher: publisher,
            auto_reconnect,
            last_failure,
            resync_nudge,
            stream_output,
        })
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state.load()
    }

    pub fn request_catalog_resync(&self) {
        self.resync_nudge.notify_one();
    }

    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.auto_reconnect.send_replace(enabled);
    }

    pub fn auto_reconnect_enabled(&self) -> bool {
        *self.auto_reconnect.borrow()
    }

    pub(crate) async fn active_session(&self) -> Result<LiveSession, ObsError> {
        let guard = self.inner.read().await;
        guard.clone().ok_or(ObsError::Disconnected)
    }

    #[cfg(test)]
    pub fn new_for_test(endpoint: String) -> Self {
        let (host, port) =
            parse_endpoint(&endpoint).unwrap_or(("localhost".to_owned(), DEFAULT_OBS_PORT));
        let (health_tx, health_state) = make_health_channel();
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected));
        let stream_output = StreamOutputFeed::new(Arc::clone(&state), Arc::clone(&health_state));
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(None)),
            endpoint,
            state,
            shutdown: Arc::new(tokio::sync::Mutex::new(Arc::new(Notify::new()))),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            obs_id: IntegrationId::new(OBS_PLATFORM_ID),
            obs_version: Arc::new(OnceLock::new()),
            obs_ws_version: Arc::new(RwLock::new(None)),
            health_state,
            health_tx,
            catalog_state: Arc::new(RwLock::new(ObsCatalog::default())),
            option_revisions: Arc::new(CollectionRevisionSignal::new()),
            scene_item_id_cache: Arc::new(Mutex::new(SceneItemIds::default())),
            reconnect_host: host,
            reconnect_port: port,
            reconnect_password: Arc::new(None),
            reconnect_publisher: Arc::new(crate::runners::test_support::NoopPublisher),
            auto_reconnect: Arc::new(watch::Sender::new(true)),
            last_failure: Arc::new(RwLock::new(None)),
            resync_nudge: Arc::new(Notify::new()),
            stream_output,
        }
    }
}

impl Drop for ObsClient {
    fn drop(&mut self) {
        if let Ok(shutdown) = self.shutdown.try_lock() {
            shutdown.notify_one();
        }
        if let Ok(mut supervisor) = self.supervisor.lock()
            && let Some(handle) = supervisor.take()
        {
            handle.abort();
        }
    }
}

impl BuiltinStatus for ObsClient {
    fn id(&self) -> &IntegrationId {
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

    fn connect_failure(&self) -> Option<MessageRef> {
        self.last_failure.read().ok().and_then(|g| g.clone())
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

    fn name_badges(&self) -> Vec<HeroBadge> {
        let Some(ws_version) = read_lock(&self.obs_ws_version).clone() else {
            return Vec::new();
        };
        let version = VerbatimText::new(ws_version);
        let label = match self.uptime() {
            Some(uptime) => MessageRef::new(labels::WS_BADGE_UPTIME)
                .with_arg("version", version)
                .with_arg(
                    "uptime",
                    VerbatimText::new(crate::health::format_duration_hm(uptime)),
                ),
            None => MessageRef::new(labels::WS_BADGE).with_arg("version", version),
        };
        vec![HeroBadge {
            label: label.into(),
            tone: HeroBadgeTone::Neutral,
            monospace: true,
        }]
    }
}

#[async_trait]
impl BuiltinControl for ObsClient {
    async fn reconnect(&self) -> ControlOutcome {
        let mut slot = self.shutdown.lock().await;
        let old_notify = slot.clone();
        old_notify.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle
            && let Err(error) = h.await
        {
            tracing::warn!(%error, "obs supervisor task ended abnormally");
        }

        let new_notify = Arc::new(Notify::new());
        *slot = Arc::clone(&new_notify);
        drop(slot);

        settle_connection_state(
            &self.state,
            &*self.reconnect_publisher,
            &self.stream_output,
            ConnectionState::Connecting,
            ConnectionTransition::Expected,
        );
        clear_connected_at(&self.connected_at);
        record_failure(&self.last_failure, None);

        let ctx = SupervisorContext {
            inner: Arc::clone(&self.inner),
            state: Arc::clone(&self.state),
            shutdown: new_notify,
            connected_at: Arc::clone(&self.connected_at),
            obs_version: Arc::clone(&self.obs_version),
            obs_ws_version: Arc::clone(&self.obs_ws_version),
            catalog_state: Arc::clone(&self.catalog_state),
            option_revisions: Arc::clone(&self.option_revisions),
            health_state: Arc::clone(&self.health_state),
            health_tx: self.health_tx.clone(),
            publisher: Arc::clone(&self.reconnect_publisher),
            item_cache: Arc::clone(&self.scene_item_id_cache),
            auto_reconnect: self.auto_reconnect.subscribe(),
            last_failure: Arc::clone(&self.last_failure),
            resync_nudge: Arc::clone(&self.resync_nudge),
            stream_output: self.stream_output.clone(),
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
        if let Some(h) = handle
            && let Err(error) = h.await
        {
            tracing::warn!(%error, "obs supervisor task ended abnormally");
        }
        record_failure(&self.last_failure, None);
        settle_connection_state(
            &self.state,
            &*self.reconnect_publisher,
            &self.stream_output,
            ConnectionState::Disconnected,
            ConnectionTransition::Expected,
        );
        Ok(())
    }

    async fn refresh_token(&self) -> ControlOutcome {
        Err(ControlFailure::Unsupported)
    }
}

struct SupervisorContext {
    inner: SessionSlot,
    state: Arc<AtomicConnectionState>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    obs_version: Arc<OnceLock<String>>,
    obs_ws_version: Arc<RwLock<Option<String>>>,
    catalog_state: Arc<RwLock<ObsCatalog>>,
    option_revisions: Arc<CollectionRevisionSignal>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    publisher: Arc<dyn EventPublisher>,
    item_cache: Arc<Mutex<SceneItemIds>>,
    auto_reconnect: watch::Receiver<bool>,
    last_failure: Arc<RwLock<Option<MessageRef>>>,
    resync_nudge: Arc<Notify>,
    stream_output: StreamOutputFeed,
}

const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

const EVENT_BUFFER_CAPACITY: usize = 1024;

enum RetryWait {
    Elapsed,
    Disabled,
    Shutdown,
}

async fn wait_before_retry(
    delay: Duration,
    shutdown: &Notify,
    auto_reconnect: &mut watch::Receiver<bool>,
) -> RetryWait {
    tokio::select! {
        () = tokio::time::sleep(delay) => RetryWait::Elapsed,
        () = shutdown.notified() => RetryWait::Shutdown,
        _ = auto_reconnect.wait_for(|enabled| !*enabled) => RetryWait::Disabled,
    }
}

async fn park_until_auto_reconnect(
    shutdown: &Notify,
    auto_reconnect: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        () = shutdown.notified() => false,
        resumed = auto_reconnect.wait_for(|enabled| *enabled) => resumed.is_ok(),
    }
}

fn record_failure(slot: &RwLock<Option<MessageRef>>, failure: Option<MessageRef>) {
    if let Ok(mut g) = slot.write() {
        *g = failure;
    }
}

async fn run_supervisor(host: String, port: u16, password: Option<String>, ctx: SupervisorContext) {
    let SupervisorContext {
        inner,
        state,
        shutdown,
        connected_at,
        obs_version,
        obs_ws_version,
        catalog_state,
        option_revisions,
        health_state,
        health_tx,
        publisher,
        item_cache,
        mut auto_reconnect,
        last_failure,
        resync_nudge,
        stream_output,
    } = ctx;
    let mut backoff = Backoff::new(RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY);
    let mut reconnecting = false;

    loop {
        if reconnecting {
            let delay = backoff.next_delay();
            let retry_enabled = *auto_reconnect.borrow_and_update();
            let wait = if retry_enabled {
                tracing::info!(
                    host = %host,
                    port,
                    delay_ms = delay.as_millis(),
                    "reconnecting to OBS"
                );
                wait_before_retry(delay, &shutdown, &mut auto_reconnect).await
            } else {
                RetryWait::Disabled
            };
            match wait {
                RetryWait::Elapsed => {}
                RetryWait::Shutdown => {
                    settle_connection_state(
                        &state,
                        &*publisher,
                        &stream_output,
                        ConnectionState::Disconnected,
                        ConnectionTransition::Expected,
                    );
                    return;
                }
                RetryWait::Disabled => {
                    tracing::info!(host = %host, port, "OBS auto-reconnect is off; waiting for a manual connect");
                    settle_connection_state(
                        &state,
                        &*publisher,
                        &stream_output,
                        ConnectionState::Disconnected,
                        ConnectionTransition::Unexpected,
                    );
                    if !park_until_auto_reconnect(&shutdown, &mut auto_reconnect).await {
                        return;
                    }
                    backoff.reset();
                }
            }
        }

        settle_connection_state(
            &state,
            &*publisher,
            &stream_output,
            if reconnecting {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Connecting
            },
            ConnectionTransition::Unexpected,
        );
        tracing::debug!(host = %host, port, "attempting OBS connection");

        let connect_config = obws::client::ConnectConfig {
            host: host.as_str(),
            port,
            password: password.as_deref(),
            event_subscriptions: Some(required_event_subscriptions()),
            broadcast_capacity: EVENT_BUFFER_CAPACITY,
            connect_timeout: obws::client::DEFAULT_CONNECT_TIMEOUT,
            dangerous: None,
        };

        let attempt = tokio::select! {
            outcome = obws::Client::connect_with_config(connect_config) => outcome.map_err(|e| {
                let message = connect_failure_message(&e);
                (message, map_obws_error(e))
            }),
            () = shutdown.notified() => {
                settle_connection_state(&state, &*publisher, &stream_output, ConnectionState::Disconnected, ConnectionTransition::Expected);
                return;
            }
        };

        let client = match attempt {
            Ok(client) => client,
            Err((message, ObsError::Authentication)) => {
                tracing::warn!(host = %host, port, "OBS authentication rejected");
                record_failure(&last_failure, Some(message));
                settle_connection_state(
                    &state,
                    &*publisher,
                    &stream_output,
                    ConnectionState::Disconnected,
                    ConnectionTransition::Unexpected,
                );
                publisher.publish(crate::events::make_connection_auth_failed(
                    "authentication rejected",
                ));
                return;
            }
            Err((message, e)) => {
                tracing::debug!(host = %host, port, error = %e, "OBS connection attempt failed");
                record_failure(&last_failure, Some(message));
                reconnecting = true;
                continue;
            }
        };

        let session = LiveSession::new(client);
        let mut stream = match session.obs().events() {
            Ok(stream) => stream,
            Err(e) => {
                tracing::info!(
                    host = %host,
                    port,
                    error = %e,
                    "OBS closed the connection right after the handshake"
                );
                record_failure(&last_failure, Some(messages::connection_lost()));
                reconnecting = true;
                continue;
            }
        };

        let primed = tokio::select! {
            outcome = prime_session(
                &session,
                &obs_version,
                &obs_ws_version,
                &catalog_state,
                &health_state,
                &health_tx,
                &item_cache,
            ) => outcome,
            () = shutdown.notified() => {
                settle_connection_state(&state, &*publisher, &stream_output, ConnectionState::Disconnected, ConnectionTransition::Expected);
                return;
            }
        };
        if let Err(e) = primed {
            tracing::info!(
                host = %host,
                port,
                error = %e,
                "OBS connection lost while loading its catalog"
            );
            record_failure(
                &last_failure,
                Some(match e {
                    ObsError::Timeout => messages::unresponsive(),
                    _ => messages::connection_lost(),
                }),
            );
            reconnecting = true;
            continue;
        }

        inner.write().await.replace(session.clone());

        if let Ok(mut g) = connected_at.write() {
            *g = Some(OffsetDateTime::now_utc());
        }

        record_failure(&last_failure, None);
        settle_connection_state(
            &state,
            &*publisher,
            &stream_output,
            ConnectionState::Connected,
            ConnectionTransition::Unexpected,
        );
        tracing::info!(host = %host, port, "connected to OBS");
        publisher.publish(crate::events::make_connection_connected());
        option_revisions.bump();

        let feeds = CatalogFeeds {
            catalog_state: Arc::clone(&catalog_state),
            option_revisions: Arc::clone(&option_revisions),
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            item_cache: Arc::clone(&item_cache),
            stream_output: stream_output.clone(),
        };
        let stats_handle = AbortOnDrop(spawn_stats_poll(
            Arc::clone(&inner),
            Arc::clone(&resync_nudge),
            feeds.clone(),
        ));

        let mut pending_resync: Option<AbortOnDrop> = None;

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    pending_resync.take();
                    stats_handle.abort();
                    inner.write().await.take();
                    clear_connected_at(&connected_at);
                    *write_lock(&obs_ws_version) = None;
                    settle_connection_state(&state, &*publisher, &stream_output, ConnectionState::Disconnected, ConnectionTransition::Expected);
                    option_revisions.bump();
                    tracing::info!("OBS supervisor shutting down");
                    return;
                }
                () = session.suspected() => {
                    tracing::warn!(host = %host, port, "OBS stopped answering requests; reconnecting");
                    record_failure(&last_failure, Some(messages::unresponsive()));
                    break;
                }
                item = stream.next() => match item {
                    Some(ev) => {
                        handle_obs_event(
                            &ev,
                            &catalog_state,
                            &health_state,
                            &health_tx,
                            &item_cache,
                            &*publisher,
                        );
                        stream_output.refresh();
                        if changes_option_lists(&ev) {
                            option_revisions.bump();
                        }
                        if matches!(ev, obws::events::Event::CurrentSceneCollectionChanged { .. }) {
                            respawn_full_resync(&mut pending_resync, &session, &feeds);
                        }
                    }
                    None if session.obs().events().is_ok() => {
                        tracing::warn!("OBS events overflowed the buffer; resyncing the catalog");
                        respawn_full_resync(&mut pending_resync, &session, &feeds);
                    }
                    None => {
                        tracing::info!(host = %host, port, "OBS connection lost; reconnecting");
                        record_failure(&last_failure, Some(messages::connection_lost()));
                        break;
                    }
                },
            }
        }

        pending_resync.take();
        stats_handle.abort();
        inner.write().await.take();
        clear_connected_at(&connected_at);
        *write_lock(&obs_ws_version) = None;
        option_revisions.bump();
        let retry = *auto_reconnect.borrow_and_update();
        settle_connection_state(
            &state,
            &*publisher,
            &stream_output,
            if retry {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Disconnected
            },
            ConnectionTransition::Unexpected,
        );
        publisher.publish(crate::events::make_connection_disconnected(
            crate::vocabulary::fields::connection::reason::CONNECTION_LOST,
            None,
        ));
        backoff.reset();
        reconnecting = true;
    }
}

async fn prime_session(
    session: &LiveSession,
    obs_version: &OnceLock<String>,
    obs_ws_version: &RwLock<Option<String>>,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    item_cache: &Mutex<SceneItemIds>,
) -> Result<(), ObsError> {
    match session
        .request("GetVersion", session.obs().general().version())
        .await
    {
        Ok(v) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the version reported on the first connect is kept"
            )]
            let _ = obs_version.set(v.obs_studio_version.to_string());
            *write_lock(obs_ws_version) = Some(v.obs_web_socket_version.to_string());
        }
        Err(e) if e.is_connection_loss() => return Err(e),
        Err(e) => tracing::warn!(error = %e, "failed to fetch OBS version"),
    }
    snapshot_catalog(session, catalog_state, health_state, health_tx, item_cache).await
}

#[derive(Clone)]
struct CatalogFeeds {
    catalog_state: Arc<RwLock<ObsCatalog>>,
    option_revisions: Arc<CollectionRevisionSignal>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    item_cache: Arc<Mutex<SceneItemIds>>,
    stream_output: StreamOutputFeed,
}

fn respawn_full_resync(
    pending: &mut Option<AbortOnDrop>,
    session: &LiveSession,
    feeds: &CatalogFeeds,
) {
    pending.take();
    let session = session.clone();
    let CatalogFeeds {
        catalog_state,
        option_revisions,
        health_state,
        health_tx,
        item_cache,
        stream_output,
    } = feeds.clone();
    *pending = Some(AbortOnDrop(tokio::spawn(async move {
        if let Err(e) = snapshot_catalog(
            &session,
            &catalog_state,
            &health_state,
            &health_tx,
            &item_cache,
        )
        .await
        {
            tracing::debug!(error = %e, "OBS catalog resync interrupted");
        }
        option_revisions.bump();
        stream_output.refresh();
    })));
}

fn changes_option_lists(ev: &obws::events::Event) -> bool {
    use obws::events::Event;
    matches!(
        ev,
        Event::SceneCreated { .. }
            | Event::SceneRemoved { .. }
            | Event::SceneNameChanged { .. }
            | Event::SceneListChanged { .. }
            | Event::CurrentProgramSceneChanged { .. }
            | Event::InputCreated { .. }
            | Event::InputRemoved { .. }
            | Event::InputNameChanged { .. }
            | Event::SceneItemCreated { .. }
            | Event::SceneItemRemoved { .. }
            | Event::SceneItemEnableStateChanged { .. }
            | Event::CurrentSceneTransitionChanged { .. }
            | Event::CurrentProfileChanged { .. }
            | Event::ProfileListChanged { .. }
            | Event::CurrentSceneCollectionChanged { .. }
            | Event::SceneCollectionListChanged { .. }
    )
}

fn option_fingerprint(catalog_state: &RwLock<ObsCatalog>) -> Option<u64> {
    catalog_state
        .read()
        .ok()
        .map(|catalog| catalog.option_fingerprint())
}

fn handle_obs_event(
    ev: &obws::events::Event,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    item_cache: &Mutex<SceneItemIds>,
    publisher: &dyn EventPublisher,
) {
    let (from_scene, visibility, lock) = {
        let mut catalog = write_lock(catalog_state);
        let mut ids = item_cache.lock().unwrap_or_else(PoisonError::into_inner);

        let from_scene = match ev {
            obws::events::Event::CurrentProgramSceneChanged { .. } => catalog.current_scene.clone(),
            obws::events::Event::CurrentPreviewSceneChanged { .. } => {
                catalog.current_preview_scene.clone()
            }
            _ => None,
        };
        let mut visibility = None;
        let mut lock = None;

        crate::events::apply_catalog_update(ev, &mut catalog);
        crate::events::apply_item_id_update(ev, &mut ids);

        match ev {
            obws::events::Event::SceneItemEnableStateChanged {
                scene,
                item_id,
                enabled,
                ..
            } => {
                if let Some(name) = ids.source_name(&scene.name, *item_id) {
                    if let Some(info) = catalog
                        .sources
                        .get_mut(&scene.name)
                        .and_then(|sources| sources.iter_mut().find(|s| s.name == name))
                    {
                        info.visible = *enabled;
                    }
                    catalog.revision = catalog.revision.wrapping_add(1);
                    visibility = Some(crate::events::map_scene_item_visibility(
                        &scene.name,
                        &name,
                        *enabled,
                    ));
                }
            }
            obws::events::Event::SceneItemLockStateChanged {
                scene,
                item_id,
                locked,
                ..
            } => {
                if let Some(name) = ids.source_name(&scene.name, *item_id) {
                    if let Some(info) = catalog
                        .sources
                        .get_mut(&scene.name)
                        .and_then(|sources| sources.iter_mut().find(|s| s.name == name))
                    {
                        info.locked = *locked;
                    }
                    catalog.revision = catalog.revision.wrapping_add(1);
                    lock = Some(crate::events::map_scene_item_lock(
                        &scene.name,
                        &name,
                        *locked,
                    ));
                }
            }
            obws::events::Event::SceneItemCreated { scene, source, .. } => {
                let known_kind = catalog
                    .sources
                    .values()
                    .flatten()
                    .find(|s| s.name == source.name && s.kind.is_some())
                    .and_then(|s| s.kind.clone());
                if let Some(kind) = known_kind
                    && let Some(info) = catalog
                        .sources
                        .get_mut(&scene.name)
                        .and_then(|items| items.iter_mut().find(|s| s.name == source.name))
                {
                    info.kind = Some(kind);
                }
            }
            _ => {}
        }
        (from_scene, visibility, lock)
    };

    let deltas = if let Ok(mut health) = health_state.write() {
        crate::events::apply_health_update(ev, &mut health)
    } else {
        vec![]
    };
    for delta in deltas {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "having no subscribers is a normal state"
        )]
        let _ = health_tx.send(delta);
    }

    if let Some(bus_event) = crate::events::map_obs_event(ev, from_scene.as_deref()) {
        publisher.publish(bus_event);
    }
    for event in visibility.into_iter().chain(lock) {
        publisher.publish(event);
    }
}

fn tolerate_rejection<T>(outcome: Result<T, ObsError>) -> Result<Option<T>, ObsError> {
    match outcome {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.is_connection_loss() => Err(e),
        Err(_) => Ok(None),
    }
}

async fn snapshot_catalog(
    session: &LiveSession,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    item_cache: &Mutex<SceneItemIds>,
) -> Result<(), ObsError> {
    for _ in 0..TOPOLOGY_FETCH_ATTEMPTS {
        if snapshot_catalog_once(session, catalog_state, health_state, health_tx, item_cache)
            .await?
        {
            return Ok(());
        }
    }
    tracing::warn!(
        "OBS catalog kept changing during every resync attempt; keeping the live mirror"
    );
    Ok(())
}

async fn snapshot_catalog_once(
    session: &LiveSession,
    catalog_state: &RwLock<ObsCatalog>,
    health_state: &RwLock<HealthSnapshot>,
    health_tx: &broadcast::Sender<HealthDelta>,
    item_cache: &Mutex<SceneItemIds>,
) -> Result<bool, ObsError> {
    use obws::requests::inputs::InputId;
    use obws::requests::scenes::SceneId;

    let obs = session.obs();
    let started_at = read_lock(catalog_state).revision;

    let scenes: Vec<String> =
        tolerate_rejection(session.request("GetSceneList", obs.scenes().list()).await)?
            .map(|list| list.scenes.iter().map(|s| s.id.name.clone()).collect())
            .unwrap_or_default();

    let current_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentProgramScene",
                obs.scenes().current_program_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let current_preview_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentPreviewScene",
                obs.scenes().current_preview_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> = HashMap::new();
    let mut db_cache: HashMap<String, f32> = HashMap::new();
    let mut fresh_item_cache = SceneItemIds::default();
    for scene in &scenes {
        let Some(items) = tolerate_rejection(
            session
                .request(
                    "GetSceneItemList",
                    obs.scene_items().list(SceneId::Name(scene)),
                )
                .await,
        )?
        else {
            continue;
        };
        let mut infos = Vec::with_capacity(items.len());
        for item in items {
            fresh_item_cache.insert(scene, item.id, &item.source_name);

            let visible = tolerate_rejection(
                session
                    .request(
                        "GetSceneItemEnabled",
                        obs.scene_items().enabled(SceneId::Name(scene), item.id),
                    )
                    .await,
            )?
            .unwrap_or(true);
            let locked = tolerate_rejection(
                session
                    .request(
                        "GetSceneItemLocked",
                        obs.scene_items().locked(SceneId::Name(scene), item.id),
                    )
                    .await,
            )?
            .unwrap_or(false);

            let kind = item.input_kind;
            let audio_db = if crate::catalog::is_audio_kind(kind.as_deref()) {
                match db_cache.get(&item.source_name) {
                    Some(db) => Some(*db),
                    None => {
                        let fetched = tolerate_rejection(
                            session
                                .request(
                                    "GetInputVolume",
                                    obs.inputs().volume(InputId::Name(&item.source_name)),
                                )
                                .await,
                        )?
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

    let audio_inputs: Vec<String> = tolerate_rejection(
        session
            .request("GetInputList", obs.inputs().list(None))
            .await,
    )?
    .map(|inputs| inputs.into_iter().map(|i| i.id.name).collect())
    .unwrap_or_default();

    let stream_status = tolerate_rejection(
        session
            .request("GetStreamStatus", obs.streaming().status())
            .await,
    )?;
    let record_status = tolerate_rejection(
        session
            .request("GetRecordStatus", obs.recording().status())
            .await,
    )?;

    {
        let mut catalog = write_lock(catalog_state);
        if catalog.revision != started_at {
            return Ok(false);
        }
        catalog.scenes = scenes;
        catalog.audio_inputs = audio_inputs;
        catalog.sources = sources_by_scene;
        catalog.current_scene = current_scene;
        catalog.current_preview_scene = current_preview_scene;
        *item_cache.lock().unwrap_or_else(PoisonError::into_inner) = fresh_item_cache;
    }

    if let Some(status) = stream_status
        && let Ok(mut snapshot) = health_state.write()
    {
        for delta in crate::events::apply_stream_status_update(&status, &mut snapshot) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "having no subscribers is a normal state"
            )]
            let _ = health_tx.send(delta);
        }
    }
    if let Some(status) = record_status
        && let Ok(mut snapshot) = health_state.write()
    {
        for delta in crate::events::apply_record_status_update(&status, &mut snapshot) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "having no subscribers is a normal state"
            )]
            let _ = health_tx.send(delta);
        }
    }
    Ok(true)
}

async fn reconcile_catalog_topology(
    session: &LiveSession,
    catalog_state: &RwLock<ObsCatalog>,
    item_cache: &Mutex<SceneItemIds>,
) -> Result<(), ObsError> {
    for _ in 0..TOPOLOGY_FETCH_ATTEMPTS {
        if reconcile_catalog_topology_once(session, catalog_state, item_cache).await? {
            return Ok(());
        }
    }
    tracing::warn!(
        "OBS catalog kept changing during every reconcile attempt; keeping the live mirror"
    );
    Ok(())
}

async fn reconcile_catalog_topology_once(
    session: &LiveSession,
    catalog_state: &RwLock<ObsCatalog>,
    item_cache: &Mutex<SceneItemIds>,
) -> Result<bool, ObsError> {
    use obws::requests::scenes::SceneId;

    let obs = session.obs();
    let started_at = read_lock(catalog_state).revision;

    let scenes: Vec<String> =
        tolerate_rejection(session.request("GetSceneList", obs.scenes().list()).await)?
            .map(|list| list.scenes.iter().map(|s| s.id.name.clone()).collect())
            .unwrap_or_default();

    let fetched_current_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentProgramScene",
                obs.scenes().current_program_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let fetched_current_preview_scene: Option<String> = tolerate_rejection(
        session
            .request(
                "GetCurrentPreviewScene",
                obs.scenes().current_preview_scene(),
            )
            .await,
    )?
    .map(|s| s.id.name);

    let audio_inputs: Vec<String> = tolerate_rejection(
        session
            .request("GetInputList", obs.inputs().list(None))
            .await,
    )?
    .map(|inputs| inputs.into_iter().map(|i| i.id.name).collect())
    .unwrap_or_default();

    let mut fetched_topology: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
    let mut fresh_item_cache = SceneItemIds::default();
    for scene in &scenes {
        let Some(items) = tolerate_rejection(
            session
                .request(
                    "GetSceneItemList",
                    obs.scene_items().list(SceneId::Name(scene)),
                )
                .await,
        )?
        else {
            continue;
        };
        let mut entries = Vec::with_capacity(items.len());
        for item in items {
            fresh_item_cache.insert(scene, item.id, &item.source_name);
            entries.push((item.source_name, item.input_kind));
        }
        fetched_topology.insert(scene.clone(), entries);
    }

    let mut catalog = write_lock(catalog_state);
    if catalog.revision != started_at {
        return Ok(false);
    }
    merge_reconciled_topology(
        &mut catalog,
        FetchedTopology {
            scenes,
            audio_inputs,
            sources: fetched_topology,
            current_scene: fetched_current_scene,
            current_preview_scene: fetched_current_preview_scene,
        },
    );
    *item_cache.lock().unwrap_or_else(PoisonError::into_inner) = fresh_item_cache;
    Ok(true)
}

pub(crate) struct FetchedTopology {
    pub(crate) scenes: Vec<String>,
    pub(crate) audio_inputs: Vec<String>,
    pub(crate) sources: HashMap<String, Vec<(String, Option<String>)>>,
    pub(crate) current_scene: Option<String>,
    pub(crate) current_preview_scene: Option<String>,
}

pub(crate) fn merge_reconciled_topology(catalog: &mut ObsCatalog, fetched: FetchedTopology) {
    let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> =
        HashMap::with_capacity(fetched.sources.len());
    for (scene, entries) in fetched.sources {
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

    catalog.scenes = fetched.scenes;
    catalog.audio_inputs = fetched.audio_inputs;
    catalog.sources = sources_by_scene;
    catalog.current_scene = fetched.current_scene;
    catalog.current_preview_scene = fetched.current_preview_scene;
}

const STATS_POLL_INTERVAL: Duration = Duration::from_secs(2);
const STATS_POLL_REQUEST_TIMEOUT: Duration = STATS_POLL_INTERVAL;
const UNANSWERED_STATS_POLLS_BEFORE_SUSPECT: u32 = 3;
const TOPOLOGY_FETCH_ATTEMPTS: u32 = 5;

fn spawn_stats_poll(
    inner: SessionSlot,
    resync_nudge: Arc<Notify>,
    feeds: CatalogFeeds,
) -> JoinHandle<()> {
    let CatalogFeeds {
        catalog_state,
        option_revisions,
        health_state,
        health_tx,
        item_cache,
        stream_output,
    } = feeds;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(STATS_POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut unanswered_polls: u32 = 0;
        loop {
            let nudged = tokio::select! {
                _ = ticker.tick() => false,
                () = resync_nudge.notified() => true,
            };

            let session = {
                let guard = inner.read().await;
                let Some(session) = guard.as_ref() else {
                    continue;
                };
                session.clone()
            };

            if !nudged {
                let general = session.obs().general();
                let streaming = session.obs().streaming();
                let recording = session.obs().recording();
                let (stats, stream_status, record_status) = tokio::join!(
                    with_deadline(STATS_POLL_REQUEST_TIMEOUT, "GetStats", general.stats()),
                    with_deadline(
                        STATS_POLL_REQUEST_TIMEOUT,
                        "GetStreamStatus",
                        streaming.status()
                    ),
                    with_deadline(
                        STATS_POLL_REQUEST_TIMEOUT,
                        "GetRecordStatus",
                        recording.status()
                    ),
                );

                let unanswered = matches!(stats, Err(ObsError::Timeout))
                    || matches!(stream_status, Err(ObsError::Timeout))
                    || matches!(record_status, Err(ObsError::Timeout));
                if unanswered {
                    unanswered_polls += 1;
                    if unanswered_polls >= UNANSWERED_STATS_POLLS_BEFORE_SUSPECT {
                        unanswered_polls = 0;
                        session.mark_suspect();
                    }
                } else {
                    unanswered_polls = 0;
                }

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
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "having no subscribers is a normal state"
                    )]
                    let _ = health_tx.send(delta);
                }
                stream_output.refresh();
            }

            if nudged {
                let before = option_fingerprint(&catalog_state);
                if let Err(e) =
                    reconcile_catalog_topology(&session, &catalog_state, &item_cache).await
                {
                    tracing::debug!(error = %e, "OBS catalog reconciliation interrupted");
                }
                if option_fingerprint(&catalog_state) != before {
                    option_revisions.bump();
                }
            }
        }
    })
}

const DEFAULT_OBS_PORT: u16 = 4455;

pub fn parse_endpoint(endpoint: &str) -> Result<(String, u16), ObsError> {
    let invalid = || ObsError::Connect(format!("invalid endpoint '{endpoint}'"));
    let trimmed = endpoint.trim();
    if trimmed.starts_with("wss://") {
        return Err(ObsError::Connect(
            "TLS endpoints (wss://) are not supported".to_owned(),
        ));
    }
    let without_scheme = trimmed.strip_prefix("ws://").unwrap_or(trimmed);
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();

    let (host, port_text) = if authority.starts_with('[') {
        let close = authority.find(']').ok_or_else(invalid)?;
        let (host, rest) = authority.split_at(close + 1);
        let port_text = match rest {
            "" => None,
            rest => Some(rest.strip_prefix(':').ok_or_else(invalid)?),
        };
        (host, port_text)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port_text)) => (host, Some(port_text)),
            None => (authority, None),
        }
    };

    if host.is_empty() || (!host.starts_with('[') && host.contains(':')) {
        return Err(invalid());
    }
    let port = match port_text {
        Some(text) => text
            .parse::<u16>()
            .map_err(|_| ObsError::Connect(format!("invalid port in endpoint '{endpoint}'")))?,
        None => DEFAULT_OBS_PORT,
    };
    Ok((host.to_owned(), port))
}

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

    use forge_events::{PlatformConnectionChanged, TypedEvent};

    const EVERY_CONNECTION_STATE: [ConnectionState; 4] = [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
    ];

    #[test]
    fn a_state_transition_announces_the_new_state_only_when_it_actually_changed() {
        for previous in EVERY_CONNECTION_STATE {
            for next in EVERY_CONNECTION_STATE {
                for transition in [
                    ConnectionTransition::Expected,
                    ConnectionTransition::Unexpected,
                ] {
                    match connection_state_transition(previous, next, transition) {
                        None => assert_eq!(
                            previous, next,
                            "{previous:?} -> {next:?} never reached the bus"
                        ),
                        Some(event) => {
                            assert_ne!(
                                previous, next,
                                "{previous:?} -> {next:?} re-announced a state that did not change"
                            );
                            assert_eq!(event.kind, PlatformConnectionChanged::KIND);
                            assert_eq!(
                                event.payload["platform_id"].as_str(),
                                Some(OBS_PLATFORM_ID),
                                "{previous:?} -> {next:?} was announced for another platform"
                            );
                            assert_eq!(
                                event.payload["state"],
                                serde_json::to_value(next).unwrap(),
                                "{previous:?} -> {next:?} announced the wrong state"
                            );
                            assert_eq!(
                                event.payload["expected"].as_bool(),
                                Some(transition.is_expected()),
                                "{previous:?} -> {next:?} lost the {transition:?} flag"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_endpoint_drops_the_scheme_and_falls_back_to_the_obs_websocket_port() {
        for (endpoint, expected_host, expected_port) in [
            ("ws://localhost:4455", "localhost", 4455),
            ("192.168.1.10:4455", "192.168.1.10", 4455),
            ("localhost", "localhost", 4455),
            ("ws://localhost", "localhost", 4455),
            ("localhost:65535", "localhost", 65535),
            ("localhost:0", "localhost", 0),
            ("  ws://localhost:4456  ", "localhost", 4456),
            ("ws://localhost:4456/", "localhost", 4456),
            ("ws://localhost/", "localhost", 4455),
            ("localhost:4456/obs?token=x#frag", "localhost", 4456),
            ("[::1]", "[::1]", 4455),
            ("[::1]:4456", "[::1]", 4456),
            ("ws://[fe80::1]:4456/", "[fe80::1]", 4456),
        ] {
            let (host, port) = parse_endpoint(endpoint).unwrap();
            assert_eq!(
                (host.as_str(), port),
                (expected_host, expected_port),
                "{endpoint}"
            );
        }
    }

    #[test]
    fn parse_endpoint_refuses_tls_because_the_session_only_speaks_plain_websocket() {
        for endpoint in ["wss://obs.example.com:4456", "  wss://localhost"] {
            let Err(ObsError::Connect(reason)) = parse_endpoint(endpoint) else {
                panic!("{endpoint} was accepted");
            };
            assert!(reason.contains("TLS"), "{endpoint}: {reason}");
        }
    }

    #[test]
    fn parse_endpoint_rejects_an_authority_without_a_usable_host() {
        for endpoint in [
            "",
            "ws://",
            ":4455",
            "/",
            "::1",
            "::1:4455",
            "[::1",
            "[::1]4455",
            "[::1]:",
            "[::1]:notaport",
        ] {
            assert!(
                parse_endpoint(endpoint).is_err(),
                "expected {endpoint:?} to be rejected"
            );
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

    #[test]
    fn auto_reconnect_starts_enabled_and_follows_the_user_toggle() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        assert!(client.auto_reconnect_enabled());
        client.set_auto_reconnect(false);
        assert!(!client.auto_reconnect_enabled());
        client.set_auto_reconnect(true);
        assert!(client.auto_reconnect_enabled());
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
        item_cache: Mutex<SceneItemIds>,
        publisher: CapturingPublisher,
    }

    impl EventHarness {
        fn new() -> Self {
            let (health_tx, _) = broadcast::channel(16);
            Self {
                catalog: RwLock::new(ObsCatalog::default()),
                health: RwLock::new(HealthSnapshot::default()),
                health_tx,
                item_cache: Mutex::new(SceneItemIds::default()),
                publisher: CapturingPublisher::default(),
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
                cache.insert(scene, item_id, source);
            }
            self
        }

        fn with_kinded_source(self, scene: &str, source: &str, kind: &str) -> Self {
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
                        kind: Some(kind.to_owned()),
                    });
            }
            self
        }

        fn source_kind(&self, scene: &str, source: &str) -> Option<String> {
            let catalog = self.catalog.read().ok()?;
            catalog
                .sources
                .get(scene)?
                .iter()
                .find(|s| s.name == source)?
                .kind
                .clone()
        }

        fn feed(&self, ev: &obws::events::Event) {
            handle_obs_event(
                ev,
                &self.catalog,
                &self.health,
                &self.health_tx,
                &self.item_cache,
                &self.publisher,
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
    fn a_scene_collection_switch_inside_obs_forgets_every_cached_item_id() {
        for switch in [
            obws::events::Event::CurrentSceneCollectionChanging {
                name: "Collab".to_owned(),
            },
            obws::events::Event::CurrentSceneCollectionChanged {
                name: "Collab".to_owned(),
            },
        ] {
            let h = EventHarness::new()
                .with_source("Gameplay", "Webcam", 7)
                .with_source("BRB", "Logo", 3);

            h.feed(&switch);

            let cache = h.item_cache.lock().unwrap();
            assert_eq!(
                (
                    cache.item_id("Gameplay", "Webcam"),
                    cache.item_id("BRB", "Logo")
                ),
                (None, None),
                "{switch:?} left item ids of the previous collection cached"
            );
        }
    }

    #[test]
    fn a_rejected_catalog_request_falls_back_while_a_lost_connection_aborts_the_load() {
        let rejected = ObsError::Request {
            request_type: "GetSceneItemLocked".to_owned(),
            message: "No scene item was found".to_owned(),
        };
        assert!(matches!(tolerate_rejection(Ok(4)), Ok(Some(4))));
        assert!(matches!(tolerate_rejection::<i32>(Err(rejected)), Ok(None)));
        for loss in [ObsError::Timeout, ObsError::Disconnected] {
            assert!(
                tolerate_rejection::<i32>(Err(loss)).is_err(),
                "a lost connection was swallowed as a rejection"
            );
        }
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

    #[test]
    fn a_created_scene_item_inherits_the_kind_of_the_same_source_in_another_scene() {
        for _ in 0..64 {
            let h = EventHarness::new()
                .with_kinded_source("Gameplay", "Mic", "wasapi_input_capture")
                .with_kinded_source("Starting", "Overlay", "browser_source");

            h.feed(&obws::events::Event::SceneItemCreated {
                scene: scene_id("BRB"),
                source: source_id("Mic"),
                item_id: 43,
                index: 0,
            });

            assert_eq!(
                h.source_kind("BRB", "Mic").as_deref(),
                Some("wasapi_input_capture"),
            );
        }
    }

    fn live_source(
        name: &str,
        visible: bool,
        locked: bool,
        audio_db: Option<f32>,
        kind: Option<&str>,
    ) -> SourceInfo {
        SourceInfo {
            name: name.to_owned(),
            visible,
            locked,
            audio_db,
            kind: kind.map(str::to_owned),
        }
    }

    fn empty_fetch() -> FetchedTopology {
        FetchedTopology {
            scenes: Vec::new(),
            audio_inputs: Vec::new(),
            sources: HashMap::new(),
            current_scene: None,
            current_preview_scene: None,
        }
    }

    #[test]
    fn reconciliation_keeps_the_live_flags_per_source_and_resolves_the_kind_fetch_first() {
        for (label, live, fetched_kind, expected) in [
            (
                "a fetched kind overrides the live one, the flags survive",
                Some(live_source(
                    "Mic",
                    false,
                    true,
                    Some(-7.5),
                    Some("stale_kind"),
                )),
                Some("wasapi_input_capture"),
                (false, true, Some(-7.5), Some("wasapi_input_capture")),
            ),
            (
                "a kindless listing falls back to the live kind",
                Some(live_source(
                    "Mic",
                    false,
                    true,
                    Some(-7.5),
                    Some("wasapi_input_capture"),
                )),
                None,
                (false, true, Some(-7.5), Some("wasapi_input_capture")),
            ),
            (
                "neither side knows the kind",
                Some(live_source("Mic", true, false, None, None)),
                None,
                (true, false, None, None),
            ),
            (
                "a source the catalog has never seen takes the visible unlocked defaults",
                None,
                Some("browser_source"),
                (true, false, None, Some("browser_source")),
            ),
            (
                "an unknown source with a kindless listing still takes the defaults",
                None,
                None,
                (true, false, None, None),
            ),
        ] {
            let mut catalog = ObsCatalog::default();
            if let Some(row) = live {
                catalog.sources.insert("Gameplay".to_owned(), vec![row]);
            }

            merge_reconciled_topology(
                &mut catalog,
                FetchedTopology {
                    scenes: vec!["Gameplay".to_owned()],
                    sources: HashMap::from([(
                        "Gameplay".to_owned(),
                        vec![("Mic".to_owned(), fetched_kind.map(str::to_owned))],
                    )]),
                    ..empty_fetch()
                },
            );

            let merged = &catalog.sources["Gameplay"][0];
            assert_eq!(
                (
                    merged.visible,
                    merged.locked,
                    merged.audio_db,
                    merged.kind.as_deref()
                ),
                expected,
                "{label}",
            );
        }
    }

    #[test]
    fn reconciliation_replaces_the_scene_source_and_input_rosters_with_the_fetched_ones() {
        let mut catalog = ObsCatalog {
            scenes: vec!["Gameplay".to_owned(), "Deleted".to_owned()],
            audio_inputs: vec!["Old Mic".to_owned()],
            sources: HashMap::from([
                (
                    "Gameplay".to_owned(),
                    vec![
                        live_source("Mic", true, false, None, None),
                        live_source("Removed Overlay", true, false, None, None),
                    ],
                ),
                (
                    "Deleted".to_owned(),
                    vec![live_source("Webcam", true, false, None, None)],
                ),
            ]),
            ..ObsCatalog::default()
        };

        merge_reconciled_topology(
            &mut catalog,
            FetchedTopology {
                scenes: vec!["Gameplay".to_owned(), "Starting".to_owned()],
                audio_inputs: vec!["New Mic".to_owned()],
                sources: HashMap::from([("Gameplay".to_owned(), vec![("Mic".to_owned(), None)])]),
                ..empty_fetch()
            },
        );

        assert_eq!(catalog.scenes, vec!["Gameplay", "Starting"]);
        assert_eq!(catalog.audio_inputs, vec!["New Mic"]);
        assert_eq!(catalog.sources.keys().collect::<Vec<_>>(), vec!["Gameplay"]);
        assert_eq!(
            catalog.sources["Gameplay"]
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Mic"],
        );
    }

    #[test]
    fn a_reconciled_catalog_takes_both_scene_pointers_from_the_fetch() {
        for (live, fetched) in [
            (Some("Gameplay"), Some("BRB")),
            (None, Some("BRB")),
            (Some("Gameplay"), None),
        ] {
            let owned = |v: Option<&str>| v.map(str::to_owned);
            let mut catalog = ObsCatalog {
                current_scene: owned(live),
                current_preview_scene: owned(live),
                ..ObsCatalog::default()
            };

            merge_reconciled_topology(
                &mut catalog,
                FetchedTopology {
                    current_scene: owned(fetched),
                    current_preview_scene: owned(fetched),
                    ..empty_fetch()
                },
            );

            assert_eq!(catalog.current_scene.as_deref(), fetched);
            assert_eq!(catalog.current_preview_scene.as_deref(), fetched);
        }
    }

    fn scene_pointers(h: &EventHarness) -> (Option<String>, Option<String>) {
        let catalog = h.catalog.read().unwrap();
        (
            catalog.current_scene.clone(),
            catalog.current_preview_scene.clone(),
        )
    }

    fn visibility_changes(h: &EventHarness) -> usize {
        h.publisher
            .kinds()
            .iter()
            .filter(|kind| kind.as_str() == "obs.source.visibility_changed")
            .count()
    }

    fn point_both_scene_pointers_at(h: &EventHarness, scene: &str) {
        let mut catalog = h.catalog.write().unwrap();
        catalog.current_scene = Some(scene.to_owned());
        catalog.current_preview_scene = Some(scene.to_owned());
    }

    #[test]
    fn a_renamed_scene_moves_its_item_ids_and_both_scene_pointers_to_the_new_name() {
        let h = EventHarness::new().with_source("Gameplay", "Webcam", 42);
        point_both_scene_pointers_at(&h, "Gameplay");

        h.feed(&obws::events::Event::SceneNameChanged {
            uuid: Default::default(),
            old_name: "Gameplay".to_owned(),
            new_name: "Live".to_owned(),
        });
        h.feed(&enable_state_changed("Gameplay", 42, false));
        assert_eq!(
            visibility_changes(&h),
            0,
            "the item id still resolved under the old scene name"
        );
        h.feed(&enable_state_changed("Live", 42, false));

        assert_eq!(
            h.publisher.last_payload("scene_name"),
            Some(serde_json::json!("Live")),
        );
        assert_eq!(h.source_flags("Live", "Webcam"), Some((false, false)));
        assert_eq!(
            scene_pointers(&h),
            (Some("Live".to_owned()), Some("Live".to_owned()))
        );
    }

    #[test]
    fn a_rename_of_another_scene_leaves_the_scene_pointers_alone() {
        let h = EventHarness::new().with_source("Gameplay", "Webcam", 42);
        point_both_scene_pointers_at(&h, "Gameplay");

        h.feed(&obws::events::Event::SceneNameChanged {
            uuid: Default::default(),
            old_name: "BRB".to_owned(),
            new_name: "Break".to_owned(),
        });

        assert_eq!(
            scene_pointers(&h),
            (Some("Gameplay".to_owned()), Some("Gameplay".to_owned()))
        );
    }

    #[test]
    fn a_removed_scene_forgets_its_item_ids_and_clears_only_the_pointers_at_it() {
        let h = EventHarness::new()
            .with_source("Gameplay", "Webcam", 42)
            .with_source("BRB", "Logo", 43);
        point_both_scene_pointers_at(&h, "Gameplay");

        h.feed(&obws::events::Event::SceneRemoved {
            id: scene_id("BRB"),
            is_group: false,
        });
        assert_eq!(
            scene_pointers(&h),
            (Some("Gameplay".to_owned()), Some("Gameplay".to_owned())),
            "removing another scene moved a pointer"
        );

        h.feed(&obws::events::Event::SceneRemoved {
            id: scene_id("Gameplay"),
            is_group: false,
        });
        h.feed(&enable_state_changed("Gameplay", 42, false));

        assert_eq!(
            visibility_changes(&h),
            0,
            "a removed scene still resolved a cached item id"
        );
        assert_eq!(scene_pointers(&h), (None, None));
    }

    #[test]
    fn catalog_rows_offer_actions_only_while_connected() {
        use forge_platform_core::{BuiltinContent, DetailSection, TrailingToken};
        use forge_types::Variant;

        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.scenes = vec!["Gameplay".to_owned(), "BRB".to_owned()];
            catalog.current_scene = Some("Gameplay".to_owned());
            catalog.sources.insert(
                "Gameplay".to_owned(),
                vec![SourceInfo {
                    name: "Cam".to_owned(),
                    visible: true,
                    locked: false,
                    audio_db: None,
                    kind: Some("v4l2_input".to_owned()),
                }],
            );
        }
        for state in EVERY_CONNECTION_STATE {
            client.state.store(state);
            let sections = client.sections();
            let DetailSection::TwoColumnLists { left, right } = &sections[0] else {
                panic!("expected TwoColumnLists");
            };
            let connected = state == ConnectionState::Connected;
            assert_eq!(
                left.items[1].on_click.is_some(),
                connected,
                "{state:?}: the non-live scene row"
            );
            let scene_keys: Vec<&Variant> = right.items[0]
                .trailing
                .iter()
                .filter_map(|t| match t {
                    TrailingToken::ActionIcon { step, .. } => step.config.get("scene"),
                    _ => None,
                })
                .collect();
            let expected_keys = if connected { 2 } else { 0 };
            assert_eq!(
                scene_keys,
                vec![&Variant::String("Gameplay".to_owned()); expected_keys],
                "{state:?}: source toggles must target the live scene only while connected"
            );
        }
    }

    fn obs_events_that_change_a_picker_list() -> Vec<obws::events::Event> {
        use obws::events::Event;
        let input = || obws::responses::inputs::InputId {
            name: "Mic".to_owned(),
            ..Default::default()
        };
        vec![
            Event::SceneCreated {
                id: scene_id("Intro"),
                is_group: false,
            },
            Event::SceneRemoved {
                id: scene_id("Intro"),
                is_group: false,
            },
            Event::SceneNameChanged {
                uuid: Default::default(),
                old_name: "Intro".to_owned(),
                new_name: "Opening".to_owned(),
            },
            Event::SceneListChanged { scenes: Vec::new() },
            Event::CurrentProgramSceneChanged {
                id: scene_id("Gameplay"),
            },
            Event::InputCreated {
                id: input(),
                kind: "pulse_input_capture".to_owned(),
                unversioned_kind: "pulse_input_capture".to_owned(),
                caps: Default::default(),
                settings: serde_json::Value::Null,
                default_settings: serde_json::Value::Null,
            },
            Event::InputRemoved { id: input() },
            Event::InputNameChanged {
                uuid: Default::default(),
                old_name: "Mic".to_owned(),
                new_name: "Studio Mic".to_owned(),
            },
            Event::SceneItemCreated {
                scene: scene_id("Gameplay"),
                source: source_id("Mic"),
                item_id: 1,
                index: 0,
            },
            Event::SceneItemRemoved {
                scene: scene_id("Gameplay"),
                source: source_id("Mic"),
                item_id: 1,
            },
            enable_state_changed("Gameplay", 1, false),
            Event::CurrentSceneTransitionChanged {
                id: obws::responses::transitions::TransitionId {
                    name: "Fade".to_owned(),
                    ..Default::default()
                },
            },
            Event::CurrentProfileChanged {
                name: "Collab".to_owned(),
            },
            Event::ProfileListChanged {
                profiles: vec!["Default".to_owned()],
            },
            Event::CurrentSceneCollectionChanged {
                name: "Collab".to_owned(),
            },
            Event::SceneCollectionListChanged {
                collections: vec!["Default".to_owned()],
            },
        ]
    }

    #[test]
    fn every_event_that_changes_a_picker_list_bumps_the_option_revision() {
        let events = obs_events_that_change_a_picker_list();
        assert_eq!(events.len(), 16, "the table lost an event");
        for event in &events {
            assert!(changes_option_lists(event), "{event:?}");
        }
    }

    #[test]
    fn events_no_picker_list_shows_leave_the_option_revision_alone() {
        use obws::events::Event;
        let input = || obws::responses::inputs::InputId {
            name: "Mic".to_owned(),
            ..Default::default()
        };
        for event in [
            Event::CurrentPreviewSceneChanged {
                id: scene_id("BRB"),
            },
            Event::SceneItemLockStateChanged {
                scene: scene_id("Gameplay"),
                item_id: 1,
                locked: true,
            },
            Event::InputMuteStateChanged {
                id: input(),
                muted: true,
            },
            Event::InputVolumeChanged {
                id: input(),
                mul: 0.5,
                db: -6.0,
            },
            Event::StudioModeStateChanged { enabled: true },
            Event::CurrentSceneCollectionChanging {
                name: "Collab".to_owned(),
            },
        ] {
            assert!(!changes_option_lists(&event), "{event:?}");
        }
    }

    #[tokio::test]
    async fn a_connected_client_lists_sources_of_the_scene_the_form_picked() {
        use forge_platform_core::{BuiltinOptionSource, OptionListId, QuickActionFieldValue};

        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut catalog = client.catalog_state.write().unwrap();
            catalog.scenes = vec!["Gameplay".to_owned(), "BRB".to_owned()];
            catalog.current_scene = Some("Gameplay".to_owned());
            for (scene, name) in [("Gameplay", "Webcam"), ("BRB", "Logo")] {
                catalog.sources.insert(
                    scene.to_owned(),
                    vec![SourceInfo {
                        name: name.to_owned(),
                        visible: true,
                        locked: false,
                        audio_db: None,
                        kind: None,
                    }],
                );
            }
        }
        client.state.store(ConnectionState::Connected);
        let sources = OptionListId::new(crate::vocabulary::option_lists::SOURCE_LIST);
        let scene = |value: &str| {
            std::collections::BTreeMap::from([(
                crate::option_source::SCENE_FIELD.to_owned(),
                QuickActionFieldValue::Text(value.to_owned()),
            )])
        };
        for (values, expected) in [
            (std::collections::BTreeMap::new(), "Webcam"),
            (scene(""), "Webcam"),
            (scene("BRB"), "Logo"),
        ] {
            let listed: Vec<String> = client
                .resolve(&sources, &values)
                .await
                .unwrap()
                .into_iter()
                .map(|e| e.value)
                .collect();
            assert_eq!(listed, vec![expected.to_owned()], "{values:?}");
        }
    }
}
