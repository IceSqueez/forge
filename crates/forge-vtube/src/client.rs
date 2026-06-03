use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use forge_events::{Event, EventPublisher, EventSource};
use forge_platform_core::{
    BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState, HeaderAction,
};

const STATE_DISCONNECTED: u8 = 0;
const STATE_CONNECTING: u8 = 1;
pub(crate) const STATE_CONNECTED: u8 = 2;
const STATE_RECONNECTING: u8 = 3;

#[derive(Debug, Clone)]
pub struct VTubeConfig {
    pub endpoint: String,
}

impl Default for VTubeConfig {
    fn default() -> Self {
        Self {
            endpoint: "ws://127.0.0.1:8001/".to_owned(),
        }
    }
}

pub struct VTubeClient {
    config: VTubeConfig,
    vtube_id: BuiltinId,
    state: Arc<AtomicU8>,
    shutdown: Arc<Notify>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    vtube_version: Arc<OnceLock<String>>,
}

impl VTubeClient {
    pub fn connect(cfg: VTubeConfig, publisher: Arc<dyn EventPublisher>) -> Self {
        let state = Arc::new(AtomicU8::new(STATE_CONNECTING));
        let shutdown = Arc::new(Notify::new());
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let vtube_version = Arc::new(OnceLock::<String>::new());

        let ctx = SupervisorContext {
            endpoint: cfg.endpoint.clone(),
            state: Arc::clone(&state),
            shutdown: Arc::clone(&shutdown),
            connected_at: Arc::clone(&connected_at),
            publisher,
        };
        let handle = tokio::spawn(run_supervisor(ctx));

        Self {
            config: cfg,
            vtube_id: BuiltinId::new("vtube"),
            state,
            shutdown,
            supervisor: Arc::new(std::sync::Mutex::new(Some(handle))),
            connected_at,
            vtube_version,
        }
    }

    pub fn connection_state(&self) -> ConnectionState {
        match self.state.load(Ordering::Acquire) {
            STATE_CONNECTED => ConnectionState::Connected,
            STATE_CONNECTING => ConnectionState::Connecting,
            STATE_RECONNECTING => ConnectionState::Reconnecting,
            _ => ConnectionState::Disconnected,
        }
    }

    pub fn connected_at(&self) -> Option<OffsetDateTime> {
        self.connected_at.read().ok().and_then(|g| *g)
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(endpoint: impl Into<String>) -> Self {
        Self {
            config: VTubeConfig {
                endpoint: endpoint.into(),
            },
            vtube_id: BuiltinId::new("vtube"),
            state: Arc::new(AtomicU8::new(STATE_DISCONNECTED)),
            shutdown: Arc::new(Notify::new()),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            vtube_version: Arc::new(OnceLock::new()),
        }
    }
}

impl Drop for VTubeClient {
    fn drop(&mut self) {
        self.shutdown.notify_one();
    }
}

impl BuiltinStatus for VTubeClient {
    fn id(&self) -> &BuiltinId {
        &self.vtube_id
    }

    fn display_name(&self) -> &str {
        "VTube Studio"
    }

    fn version(&self) -> Option<&str> {
        self.vtube_version.get().map(|s| s.as_str())
    }

    fn connection(&self) -> ConnectionState {
        self.connection_state()
    }

    fn uptime(&self) -> Option<Duration> {
        let at = self.connected_at.read().ok().and_then(|g| *g)?;
        let elapsed = OffsetDateTime::now_utc() - at;
        if elapsed.is_positive() {
            Some(elapsed.unsigned_abs())
        } else {
            None
        }
    }

    fn endpoint(&self) -> Option<&str> {
        Some(&self.config.endpoint)
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::Reconnect, HeaderAction::Disconnect]
    }
}

pub(crate) fn compute_backoff(attempt: u32) -> Duration {
    const DELAYS_SECS: [u64; 6] = [1, 2, 4, 8, 16, 30];
    Duration::from_secs(DELAYS_SECS[attempt.min(5) as usize])
}

fn emit_connection_changed(
    publisher: &dyn EventPublisher,
    endpoint: &str,
    connected: bool,
    reason: Option<String>,
) {
    let reason_val = reason
        .map(serde_json::Value::String)
        .unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "connected": connected,
        "endpoint": endpoint,
        "reason": reason_val,
    });
    publisher.publish(Event::new(
        EventSource::VTube,
        "vtube.connection.changed",
        payload,
    ));
}

struct SupervisorContext {
    endpoint: String,
    state: Arc<AtomicU8>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    publisher: Arc<dyn EventPublisher>,
}

async fn run_supervisor(ctx: SupervisorContext) {
    let SupervisorContext {
        endpoint,
        state,
        shutdown,
        connected_at,
        publisher,
    } = ctx;

    let mut attempt: u32 = 0;

    loop {
        if attempt > 0 {
            let delay = compute_backoff(attempt - 1);
            tracing::info!(
                endpoint = %endpoint,
                attempt,
                delay_ms = delay.as_millis(),
                "reconnecting to VTube Studio"
            );
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = shutdown.notified() => {
                    state.store(STATE_DISCONNECTED, Ordering::Release);
                    emit_connection_changed(&*publisher, &endpoint, false, None);
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
        tracing::debug!(endpoint = %endpoint, attempt, "attempting VTube Studio connection");

        match tokio_tungstenite::connect_async(&endpoint).await {
            Ok((ws, _)) => {
                if let Ok(mut g) = connected_at.write() {
                    *g = Some(OffsetDateTime::now_utc());
                }
                state.store(STATE_CONNECTED, Ordering::Release);
                emit_connection_changed(&*publisher, &endpoint, true, None);
                tracing::info!(endpoint = %endpoint, "connected to VTube Studio");

                let mut stream = ws;

                loop {
                    tokio::select! {
                        () = shutdown.notified() => {
                            state.store(STATE_DISCONNECTED, Ordering::Release);
                            emit_connection_changed(&*publisher, &endpoint, false, None);
                            return;
                        }
                        msg = stream.next() => {
                            match msg {
                                None | Some(Err(_)) => {
                                    tracing::info!(
                                        endpoint = %endpoint,
                                        "VTube Studio connection closed"
                                    );
                                    break;
                                }
                                Some(Ok(_)) => {}
                            }
                        }
                    }
                }

                if let Ok(mut g) = connected_at.write() {
                    *g = None;
                }
                emit_connection_changed(&*publisher, &endpoint, false, None);
                attempt = 1;
            }
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    attempt,
                    error = %e,
                    "VTube Studio connection attempt failed"
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering as AO};

    use super::*;
    use forge_events::EventPublisher;
    use forge_platform_core::BuiltinStatus;

    pub(crate) struct MockPublisher {
        pub events: Arc<std::sync::Mutex<Vec<Event>>>,
    }

    impl MockPublisher {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                events: Arc::new(std::sync::Mutex::new(Vec::new())),
            })
        }

        pub(crate) fn publisher(self: &Arc<Self>) -> Arc<dyn EventPublisher> {
            Arc::clone(self) as Arc<dyn EventPublisher>
        }

        pub(crate) fn connected_event(&self) -> Option<Event> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .find(|e| {
                    e.kind == "vtube.connection.changed"
                        && e.payload["connected"].as_bool() == Some(true)
                })
                .cloned()
        }

        pub(crate) fn disconnected_event(&self) -> Option<Event> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .find(|e| {
                    e.kind == "vtube.connection.changed"
                        && e.payload["connected"].as_bool() == Some(false)
                })
                .cloned()
        }
    }

    impl EventPublisher for MockPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[test]
    fn new_for_test_connection_state_is_disconnected() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        assert_eq!(c.connection_state(), ConnectionState::Disconnected);
    }

    #[test]
    fn new_for_test_connected_at_is_none() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        assert!(c.connected_at().is_none());
    }

    #[test]
    fn builtin_status_id_is_vtube() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert_eq!(s.id().as_str(), "vtube");
    }

    #[test]
    fn builtin_status_display_name_is_vtube_studio() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert_eq!(s.display_name(), "VTube Studio");
    }

    #[test]
    fn builtin_status_version_none_before_connect() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert!(s.version().is_none());
    }

    #[test]
    fn builtin_status_endpoint_reflects_config() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:9001/");
        let s: &dyn BuiltinStatus = &c;
        assert_eq!(s.endpoint(), Some("ws://127.0.0.1:9001/"));
    }

    #[test]
    fn builtin_status_capability_flags_not_limited() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        let flags = s.capability_flags();
        assert!(!flags.limited);
        assert!(flags.label.is_none());
    }

    #[test]
    fn builtin_status_header_actions_contains_reconnect_and_disconnect() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        let actions = s.header_actions();
        assert!(actions.contains(&HeaderAction::Reconnect));
        assert!(actions.contains(&HeaderAction::Disconnect));
        assert!(!actions.contains(&HeaderAction::RefreshToken));
    }

    #[test]
    fn builtin_status_uptime_none_when_not_connected() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let s: &dyn BuiltinStatus = &c;
        assert!(s.uptime().is_none());
    }

    #[test]
    fn vtube_config_default_endpoint() {
        let cfg = VTubeConfig::default();
        assert_eq!(cfg.endpoint, "ws://127.0.0.1:8001/");
    }

    #[test]
    fn compute_backoff_sequence_matches_spec() {
        assert_eq!(compute_backoff(0), Duration::from_secs(1));
        assert_eq!(compute_backoff(1), Duration::from_secs(2));
        assert_eq!(compute_backoff(2), Duration::from_secs(4));
        assert_eq!(compute_backoff(3), Duration::from_secs(8));
        assert_eq!(compute_backoff(4), Duration::from_secs(16));
        assert_eq!(compute_backoff(5), Duration::from_secs(30));
        assert_eq!(compute_backoff(99), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn connect_emits_connected_event_when_server_accepts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            drop(ws);
        });

        let publisher = MockPublisher::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher());

        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(
            publisher.connected_event().is_some(),
            "expected vtube.connection.changed {{connected: true}}"
        );
    }

    #[tokio::test]
    async fn disconnect_emits_connection_changed_false() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let _ = tokio_tungstenite::accept_async(stream).await;
        });

        let publisher = MockPublisher::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher());

        tokio::time::sleep(Duration::from_millis(300)).await;

        assert!(
            publisher.disconnected_event().is_some(),
            "expected vtube.connection.changed {{connected: false}}"
        );
    }

    #[tokio::test]
    async fn client_reconnects_after_server_close() {
        let accept_count = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&accept_count);
        let (accept_tx, mut accept_rx) = tokio::sync::mpsc::channel::<()>(4);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                counter.fetch_add(1, AO::Release);
                let _ = accept_tx.send(()).await;
                let _ = tokio_tungstenite::accept_async(stream).await;
            }
        });

        let publisher = MockPublisher::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher());

        let result = tokio::time::timeout(Duration::from_secs(5), async {
            accept_rx.recv().await; // first connect
            accept_rx.recv().await; // reconnect
        })
        .await;

        assert!(result.is_ok(), "expected reconnect within 5 s");
        assert!(
            accept_count.load(AO::Acquire) >= 2,
            "expected at least 2 connection attempts"
        );
    }
}
