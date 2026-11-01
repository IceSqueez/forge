use crate::vocabulary::labels;
use forge_types::{IntegrationId, MessageRef, RecoverMutex, RecoverRwLock, UserText, VerbatimText};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use forge_events::EventPublisher;
use forge_net::Backoff;
use forge_platform_core::{
    AtomicConnectionState, AutoReconnectSwitch, AutoReconnectWatch, BuiltinControl, BuiltinStatus,
    CapabilityFlags, CollectionRevisions, ConnectionState, ConnectionTransition, ControlFailure,
    ControlOutcome, CredentialState, HeaderAction, HealthDelta, HeroBadge, HeroBadgeTone,
    ReconnectingSession, SessionEnd, record_connection_failure, run_reconnect_loop,
};

use crate::catalog::{read_lock, write_lock};
use crate::catalog_revisions::CatalogRevisions;
use crate::catalog_sync::{CatalogFeeds, CatalogSink, prime_session, snapshot_catalog};
use crate::connect_failure::{connect_failure_message, credential_state_after};
use crate::endpoint::ObsEndpoint;
#[cfg(test)]
use crate::endpoint::parse_endpoint;
use crate::error::ObsError;
use crate::event_dispatch::{changes_content_rows_only, changes_option_lists, handle_obs_event};
use crate::health::{HealthSnapshot, make_health_channel};
use crate::mirror::ObsMirror;
use crate::obws_error::map_obws_error;
use crate::session::LiveSession;
use crate::stats_poll::spawn_stats_poll;
use crate::stream_output::StreamOutputFeed;
use crate::timing::ObsTiming;
use crate::vocabulary::messages;

const OBS_PLATFORM_ID: &str = crate::vocabulary::INTEGRATION_ID;

fn settle_connection_state(
    state: &AtomicConnectionState,
    publisher: &dyn EventPublisher,
    stream_output: &StreamOutputFeed,
    next: ConnectionState,
    transition: ConnectionTransition,
) {
    let announcement = state.advance(OBS_PLATFORM_ID, next, transition);
    stream_output.refresh();
    if let Some(event) = announcement {
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

pub(crate) type SessionSlot = Arc<tokio::sync::RwLock<Option<LiveSession>>>;

#[derive(Clone)]
struct ObsTarget {
    host: String,
    port: u16,
    password: Option<String>,
}

pub struct ObsClient {
    pub(crate) inner: SessionSlot,
    pub(crate) mirror: Arc<ObsMirror>,
    endpoint: String,
    state: Arc<AtomicConnectionState>,
    shutdown: Arc<tokio::sync::Mutex<Arc<Notify>>>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    connected_at: Arc<RwLock<Option<Instant>>>,
    obs_id: IntegrationId,
    obs_version: Arc<RwLock<Option<String>>>,
    obs_ws_version: Arc<RwLock<Option<String>>>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) catalog_revisions: Arc<CatalogRevisions>,
    target: ObsTarget,
    publisher: Arc<dyn EventPublisher>,
    auto_reconnect: AutoReconnectSwitch,
    last_failure: Arc<RwLock<Option<MessageRef>>>,
    resync_nudge: Arc<Notify>,
    pub(crate) stream_output: StreamOutputFeed,
    timing: ObsTiming,
}

impl ObsClient {
    pub async fn connect(
        endpoint: &ObsEndpoint,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<Self, ObsError> {
        Self::connect_with_timing(endpoint, publisher, ObsTiming::default()).await
    }

    pub async fn connect_with_timing(
        endpoint: &ObsEndpoint,
        publisher: Arc<dyn EventPublisher>,
        timing: ObsTiming,
    ) -> Result<Self, ObsError> {
        let (host, port) = (endpoint.host.clone(), endpoint.port);

        let inner: SessionSlot = Arc::new(tokio::sync::RwLock::new(None));
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connecting));
        let notify = Arc::new(Notify::new());
        let connected_at = Arc::new(RwLock::new(None::<Instant>));
        let obs_version = Arc::new(RwLock::new(None));
        let obs_ws_version = Arc::new(RwLock::new(None::<String>));
        let mirror = Arc::new(ObsMirror::default());

        let (health_tx, health_state) = make_health_channel();
        let catalog_revisions = Arc::new(CatalogRevisions::default());

        let stored_password = endpoint.password().map(str::to_owned);
        let auto_reconnect = AutoReconnectSwitch::new(true);
        let last_failure = Arc::new(RwLock::new(None::<MessageRef>));
        let resync_nudge = Arc::new(Notify::new());
        let stream_output = StreamOutputFeed::new(Arc::clone(&state), Arc::clone(&health_state));

        let client = Self {
            inner,
            endpoint: endpoint.url(),
            state,
            shutdown: Arc::new(tokio::sync::Mutex::new(Arc::clone(&notify))),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at,
            obs_id: crate::OBS_INTEGRATION.id,
            obs_version,
            obs_ws_version,
            health_state,
            health_tx,
            mirror,
            catalog_revisions,
            target: ObsTarget {
                host,
                port,
                password: stored_password,
            },
            publisher,
            auto_reconnect,
            last_failure,
            resync_nudge,
            stream_output,
            timing,
        };
        client.launch_supervisor(notify);
        Ok(client)
    }

    fn launch_supervisor(&self, shutdown: Arc<Notify>) {
        let ctx = SupervisorContext {
            inner: Arc::clone(&self.inner),
            state: Arc::clone(&self.state),
            shutdown,
            connected_at: Arc::clone(&self.connected_at),
            obs_version: Arc::clone(&self.obs_version),
            obs_ws_version: Arc::clone(&self.obs_ws_version),
            mirror: Arc::clone(&self.mirror),
            catalog_revisions: Arc::clone(&self.catalog_revisions),
            health_state: Arc::clone(&self.health_state),
            health_tx: self.health_tx.clone(),
            publisher: Arc::clone(&self.publisher),
            auto_reconnect: self.auto_reconnect.watch(),
            last_failure: Arc::clone(&self.last_failure),
            resync_nudge: Arc::clone(&self.resync_nudge),
            stream_output: self.stream_output.clone(),
            timing: self.timing,
        };
        let handle = tokio::spawn(run_supervisor(self.target.clone(), ctx));
        *self.supervisor.lock_recover() = Some(handle);
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state.load()
    }

    pub(crate) fn request_catalog_resync(&self) {
        self.resync_nudge.notify_one();
    }

    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.auto_reconnect.set(enabled);
    }

    pub fn auto_reconnect_enabled(&self) -> bool {
        self.auto_reconnect.is_enabled()
    }

    pub(crate) async fn active_session(&self) -> Result<LiveSession, ObsError> {
        let guard = self.inner.read().await;
        guard.clone().ok_or(ObsError::NotConnected)
    }

    #[cfg(test)]
    pub fn new_for_test(endpoint: String) -> Self {
        let (host, port) = parse_endpoint(&endpoint)
            .unwrap_or(("localhost".to_owned(), crate::endpoint::DEFAULT_OBS_PORT));
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
            obs_version: Arc::new(RwLock::new(None)),
            obs_ws_version: Arc::new(RwLock::new(None)),
            health_state,
            health_tx,
            mirror: Arc::new(ObsMirror::default()),
            catalog_revisions: Arc::new(CatalogRevisions::default()),
            target: ObsTarget {
                host,
                port,
                password: None,
            },
            publisher: Arc::new(forge_events::test_support::NullPublisher),
            auto_reconnect: AutoReconnectSwitch::new(true),
            last_failure: Arc::new(RwLock::new(None)),
            resync_nudge: Arc::new(Notify::new()),
            stream_output,

            timing: ObsTiming::default(),
        }
    }
}

impl Drop for ObsClient {
    fn drop(&mut self) {
        if let Ok(shutdown) = self.shutdown.try_lock() {
            shutdown.notify_one();
        }
        if let Some(handle) = self.supervisor.lock_recover().take() {
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

    fn version(&self) -> Option<String> {
        read_lock(&self.obs_version).clone()
    }

    fn connection(&self) -> ConnectionState {
        self.connection_state()
    }

    fn connect_failure(&self) -> Option<MessageRef> {
        self.last_failure.read_recover().clone()
    }

    fn uptime(&self) -> Option<Duration> {
        self.connected_at.read_recover().map(|at| at.elapsed())
    }

    fn endpoint(&self) -> Option<UserText> {
        Some(VerbatimText::new(self.endpoint.as_str()).into())
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

    fn credential_state(&self) -> CredentialState {
        credential_state_after(self.last_failure.read_recover().as_ref())
    }

    fn status_revisions(&self) -> Option<CollectionRevisions> {
        Some(self.state.status_revisions())
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
            is_identifier: true,
        }]
    }
}

#[async_trait]
impl BuiltinControl for ObsClient {
    async fn reconnect(&self) -> ControlOutcome {
        let mut slot = self.shutdown.lock().await;
        let old_notify = slot.clone();
        old_notify.notify_one();
        let handle = self.supervisor.lock_recover().take();
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
            &*self.publisher,
            &self.stream_output,
            ConnectionState::Connecting,
            ConnectionTransition::Expected,
        );
        clear_connected_at(&self.connected_at);
        record_connection_failure(&self.state, &self.last_failure, None);

        self.launch_supervisor(new_notify);

        Ok(())
    }

    async fn disconnect(&self) -> ControlOutcome {
        let slot = self.shutdown.lock().await;
        let notify = slot.clone();
        drop(slot);
        notify.notify_one();
        let handle = self.supervisor.lock_recover().take();
        if let Some(h) = handle
            && let Err(error) = h.await
        {
            tracing::warn!(%error, "obs supervisor task ended abnormally");
        }
        record_connection_failure(&self.state, &self.last_failure, None);
        settle_connection_state(
            &self.state,
            &*self.publisher,
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
    connected_at: Arc<RwLock<Option<Instant>>>,
    obs_version: Arc<RwLock<Option<String>>>,
    obs_ws_version: Arc<RwLock<Option<String>>>,
    mirror: Arc<ObsMirror>,
    catalog_revisions: Arc<CatalogRevisions>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    publisher: Arc<dyn EventPublisher>,
    auto_reconnect: AutoReconnectWatch,
    last_failure: Arc<RwLock<Option<MessageRef>>>,
    resync_nudge: Arc<Notify>,
    stream_output: StreamOutputFeed,
    timing: ObsTiming,
}

pub const EVENT_BUFFER_CAPACITY: usize = 1024;

struct ObsSupervisor {
    target: ObsTarget,
    ctx: SupervisorContext,
}

impl ReconnectingSession for ObsSupervisor {
    async fn stop_requested(&self) {
        self.ctx.shutdown.notified().await;
    }

    fn retry_scheduled(&self, delay: Duration) {
        tracing::info!(
            host = %self.target.host,
            port = self.target.port,
            delay_ms = delay.as_millis(),
            "reconnecting to OBS"
        );
    }

    fn stopped_while_retrying(&self) {
        settle_connection_state(
            &self.ctx.state,
            &*self.ctx.publisher,
            &self.ctx.stream_output,
            ConnectionState::Disconnected,
            ConnectionTransition::Expected,
        );
    }

    fn auto_reconnect_turned_off(&self) {
        tracing::info!(host = %self.target.host, port = self.target.port, "OBS auto-reconnect is off; waiting for a manual connect");
        settle_connection_state(
            &self.ctx.state,
            &*self.ctx.publisher,
            &self.ctx.stream_output,
            ConnectionState::Disconnected,
            ConnectionTransition::Unexpected,
        );
    }

    async fn attempt(&mut self, reconnecting: bool) -> SessionEnd {
        let Self {
            target:
                ObsTarget {
                    ref host,
                    port,
                    ref password,
                },
            ctx:
                SupervisorContext {
                    ref inner,
                    ref state,
                    ref shutdown,
                    ref connected_at,
                    ref obs_version,
                    ref obs_ws_version,
                    ref mirror,
                    ref catalog_revisions,
                    ref health_state,
                    ref health_tx,
                    ref publisher,
                    ref mut auto_reconnect,
                    ref last_failure,
                    ref resync_nudge,
                    ref stream_output,
                    timing,
                },
        } = *self;

        settle_connection_state(
            state,
            &**publisher,
            stream_output,
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
            connect_timeout: timing.connect_timeout,
            dangerous: None,
        };

        let attempt = tokio::select! {
            outcome = obws::Client::connect_with_config(connect_config) => outcome.map_err(|e| {
                let message = connect_failure_message(&e);
                (message, map_obws_error(e))
            }),
            () = shutdown.notified() => {
                settle_connection_state(state, &**publisher, stream_output, ConnectionState::Disconnected, ConnectionTransition::Expected);
                return SessionEnd::Stopped;
            }
        };

        let client = match attempt {
            Ok(client) => client,
            Err((message, ObsError::Authentication)) => {
                tracing::warn!(host = %host, port, "OBS authentication rejected");
                record_connection_failure(state, last_failure, Some(message));
                settle_connection_state(
                    state,
                    &**publisher,
                    stream_output,
                    ConnectionState::Disconnected,
                    ConnectionTransition::Unexpected,
                );
                publisher.publish(crate::events::make_connection_auth_failed(
                    "authentication rejected",
                ));
                return SessionEnd::Stopped;
            }
            Err((message, e)) => {
                tracing::debug!(host = %host, port, error = %e, "OBS connection attempt failed");
                record_connection_failure(state, last_failure, Some(message));
                return SessionEnd::AttemptFailed;
            }
        };

        let session = LiveSession::new(client, timing.request_timeout);
        let mut stream = match session.obs().events() {
            Ok(stream) => stream,
            Err(e) => {
                tracing::info!(
                    host = %host,
                    port,
                    error = %e,
                    "OBS closed the connection right after the handshake"
                );
                record_connection_failure(state, last_failure, Some(messages::connection_lost()));
                return SessionEnd::AttemptFailed;
            }
        };

        let sink = CatalogSink {
            mirror,
            health_state,
            health_tx,
        };
        let primed = tokio::select! {
            outcome = prime_session(
                &session,
                obs_version,
                obs_ws_version,
                &sink,
            ) => outcome,
            () = shutdown.notified() => {
                settle_connection_state(state, &**publisher, stream_output, ConnectionState::Disconnected, ConnectionTransition::Expected);
                return SessionEnd::Stopped;
            }
        };
        if let Err(e) = primed {
            tracing::info!(
                host = %host,
                port,
                error = %e,
                "OBS connection lost while loading its catalog"
            );
            record_connection_failure(
                state,
                last_failure,
                Some(match e {
                    ObsError::Timeout => messages::unresponsive(),
                    _ => messages::connection_lost(),
                }),
            );
            return SessionEnd::AttemptFailed;
        }

        inner.write().await.replace(session.clone());

        {
            let mut g = connected_at.write_recover();
            *g = Some(Instant::now());
        }

        record_connection_failure(state, last_failure, None);
        settle_connection_state(
            state,
            &**publisher,
            stream_output,
            ConnectionState::Connected,
            ConnectionTransition::Unexpected,
        );
        tracing::info!(host = %host, port, "connected to OBS");
        publisher.publish(crate::events::make_connection_connected());
        catalog_revisions.lists_changed();

        let feeds = CatalogFeeds {
            mirror: Arc::clone(mirror),
            catalog_revisions: Arc::clone(catalog_revisions),
            health_state: Arc::clone(health_state),
            health_tx: health_tx.clone(),
            stream_output: stream_output.clone(),
        };
        let stats_handle = AbortOnDrop(spawn_stats_poll(
            Arc::clone(inner),
            Arc::clone(resync_nudge),
            feeds.clone(),
            timing,
        ));

        let mut pending_resync: Option<AbortOnDrop> = None;

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    pending_resync.take();
                    stats_handle.abort();
                    inner.write().await.take();
                    clear_connected_at(connected_at);
                    *write_lock(obs_ws_version) = None;
                    settle_connection_state(state, &**publisher, stream_output, ConnectionState::Disconnected, ConnectionTransition::Expected);
                    catalog_revisions.lists_changed();
                    tracing::info!("OBS supervisor shutting down");
                    return SessionEnd::Stopped;
                }
                () = session.suspected() => {
                    tracing::warn!(host = %host, port, "OBS stopped answering requests; reconnecting");
                    record_connection_failure(state, last_failure, Some(messages::unresponsive()));
                    break;
                }
                item = stream.next() => match item {
                    Some(ev) => {
                        handle_obs_event(
                            &ev,
                            mirror,
                            health_state,
                            health_tx,
                            &**publisher,
                        );
                        stream_output.refresh();
                        if changes_option_lists(&ev) {
                            catalog_revisions.lists_changed();
                        } else if changes_content_rows_only(&ev) {
                            catalog_revisions.rows_changed();
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
                        record_connection_failure(state, last_failure, Some(messages::connection_lost()));
                        break;
                    }
                },
            }
        }

        pending_resync.take();
        stats_handle.abort();
        inner.write().await.take();
        clear_connected_at(connected_at);
        *write_lock(obs_ws_version) = None;
        catalog_revisions.lists_changed();
        let retry = auto_reconnect.is_enabled();
        settle_connection_state(
            state,
            &**publisher,
            stream_output,
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
        SessionEnd::SessionLost
    }
}

async fn run_supervisor(target: ObsTarget, ctx: SupervisorContext) {
    let backoff = Backoff::new(
        ctx.timing.reconnect_base_delay,
        ctx.timing.reconnect_max_delay,
    );
    let auto_reconnect = ctx.auto_reconnect.clone();
    let mut supervisor = ObsSupervisor { target, ctx };
    run_reconnect_loop(&mut supervisor, backoff, auto_reconnect).await;
}

fn respawn_full_resync(
    pending: &mut Option<AbortOnDrop>,
    session: &LiveSession,
    feeds: &CatalogFeeds,
) {
    pending.take();
    let session = session.clone();
    let CatalogFeeds {
        mirror,
        catalog_revisions,
        health_state,
        health_tx,
        stream_output,
    } = feeds.clone();
    *pending = Some(AbortOnDrop(tokio::spawn(async move {
        let sink = CatalogSink {
            mirror: &mirror,
            health_state: &health_state,
            health_tx: &health_tx,
        };
        if let Err(e) = snapshot_catalog(&session, &sink).await {
            tracing::debug!(error = %e, "OBS catalog resync interrupted");
        }
        catalog_revisions.lists_changed();
        stream_output.refresh();
    })));
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

fn clear_connected_at(connected_at: &RwLock<Option<Instant>>) {
    {
        let mut g = connected_at.write_recover();
        *g = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::catalog_sync::tolerate_rejection;
    use crate::mirror::FetchedTopology;
    use crate::source::SourceInfo;
    use forge_events::test_support::CapturingPublisher;

    use crate::catalog::ObsCatalog;
    use crate::mirror::merge_reconciled_topology;
    use forge_events::{PlatformConnectionChanged, TypedEvent};

    const EVERY_CONNECTION_STATE: [ConnectionState; 4] = [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
    ];

    fn last_payload(publisher: &CapturingPublisher, field: &str) -> Option<serde_json::Value> {
        publisher
            .events()
            .last()
            .map(|event| event.payload[field].clone())
    }

    #[test]
    fn a_state_transition_announces_the_new_state_only_when_it_actually_changed() {
        for previous in EVERY_CONNECTION_STATE {
            for next in EVERY_CONNECTION_STATE {
                for transition in [
                    ConnectionTransition::Expected,
                    ConnectionTransition::Unexpected,
                ] {
                    match AtomicConnectionState::new(previous).advance(
                        OBS_PLATFORM_ID,
                        next,
                        transition,
                    ) {
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

    struct EventHarness {
        mirror: ObsMirror,
        health: RwLock<HealthSnapshot>,
        health_tx: broadcast::Sender<HealthDelta>,
        publisher: CapturingPublisher,
    }

    impl EventHarness {
        fn new() -> Self {
            let (health_tx, _) = broadcast::channel(16);
            Self {
                mirror: ObsMirror::default(),
                health: RwLock::new(HealthSnapshot::default()),
                health_tx,
                publisher: CapturingPublisher::default(),
            }
        }

        fn with_source(self, scene: &str, source: &str, item_id: i64) -> Self {
            self.mirror.edit(|catalog| {
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
            });
            self.mirror.remember_item(scene, item_id, source);
            self
        }

        fn with_kinded_source(self, scene: &str, source: &str, kind: &str) -> Self {
            self.mirror.edit(|catalog| {
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
            });
            self
        }

        fn source_kind(&self, scene: &str, source: &str) -> Option<String> {
            self.mirror.read(|catalog| {
                catalog
                    .sources
                    .get(scene)?
                    .iter()
                    .find(|s| s.name == source)?
                    .kind
                    .clone()
            })
        }

        fn feed(&self, ev: &obws::events::Event) {
            handle_obs_event(
                ev,
                &self.mirror,
                &self.health,
                &self.health_tx,
                &self.publisher,
            );
        }

        fn source_flags(&self, scene: &str, source: &str) -> Option<(bool, bool)> {
            self.mirror.read(|catalog| {
                catalog
                    .sources
                    .get(scene)?
                    .iter()
                    .find(|s| s.name == source)
                    .map(|s| (s.visible, s.locked))
            })
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
            last_payload(&h.publisher, "source_name"),
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

            assert_eq!(
                (
                    h.mirror.item_id("Gameplay", "Webcam"),
                    h.mirror.item_id("BRB", "Logo")
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
        for loss in [ObsError::Timeout, ObsError::NotConnected] {
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
            last_payload(&h.publisher, "source_name"),
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
        let catalog = h.mirror.write_for_test();
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
        let mut catalog = h.mirror.write_for_test();
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
            last_payload(&h.publisher, "scene_name"),
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
            let mut catalog = client.mirror.write_for_test();
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
    fn every_event_that_changes_a_content_row_moves_the_content_revision() {
        use obws::events::Event;
        for event in [
            Event::CurrentPreviewSceneChanged {
                id: scene_id("BRB"),
            },
            Event::SceneItemLockStateChanged {
                scene: scene_id("Gameplay"),
                item_id: 1,
                locked: true,
            },
        ] {
            assert!(!changes_option_lists(&event), "{event:?}");
            assert!(changes_content_rows_only(&event), "{event:?}");
        }
        for event in obs_events_that_change_a_picker_list() {
            assert!(!changes_content_rows_only(&event), "{event:?}");
        }
    }

    #[test]
    fn a_volume_change_moves_no_content_revision() {
        let event = obws::events::Event::InputVolumeChanged {
            id: obws::responses::inputs::InputId {
                name: "Mic".to_owned(),
                ..Default::default()
            },
            mul: 0.5,
            db: -6.0,
        };
        assert!(!changes_option_lists(&event));
        assert!(!changes_content_rows_only(&event));
    }

    #[test]
    fn a_connection_change_moves_the_status_revision_and_a_repeat_does_not() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let mut revisions = client
            .status_revisions()
            .expect("obs declares a status signal");
        let moved = |revisions: &mut CollectionRevisions| {
            futures_util::FutureExt::now_or_never(revisions.changed()).is_some()
        };

        client.state.store(ConnectionState::Disconnected);
        assert!(!moved(&mut revisions), "same state");

        client.state.store(ConnectionState::Connected);
        assert!(moved(&mut revisions), "connected");

        record_connection_failure(&client.state, &client.last_failure, None);
        assert!(!moved(&mut revisions), "no failure to clear");

        record_connection_failure(
            &client.state,
            &client.last_failure,
            Some(messages::connection_lost()),
        );
        assert!(moved(&mut revisions), "a failure to show");
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
            let mut catalog = client.mirror.write_for_test();
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
