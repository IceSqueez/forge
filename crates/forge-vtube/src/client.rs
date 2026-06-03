use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

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
    pub fn connect(cfg: VTubeConfig) -> Self {
        let state = Arc::new(AtomicU8::new(STATE_CONNECTING));
        let shutdown = Arc::new(Notify::new());
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let vtube_version = Arc::new(OnceLock::<String>::new());

        let ctx = SupervisorContext {
            endpoint: cfg.endpoint.clone(),
            state: Arc::clone(&state),
            shutdown: Arc::clone(&shutdown),
            connected_at: Arc::clone(&connected_at),
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

struct SupervisorContext {
    endpoint: String,
    state: Arc<AtomicU8>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
}

async fn run_supervisor(ctx: SupervisorContext) {
    let SupervisorContext {
        endpoint,
        state,
        shutdown,
        connected_at,
    } = ctx;

    state.store(STATE_CONNECTING, Ordering::Release);

    let ws = match tokio_tungstenite::connect_async(&endpoint).await {
        Ok((ws, _)) => ws,
        Err(e) => {
            tracing::debug!(endpoint = %endpoint, error = %e, "VTube Studio connection failed");
            state.store(STATE_DISCONNECTED, Ordering::Release);
            return;
        }
    };

    if let Ok(mut g) = connected_at.write() {
        *g = Some(OffsetDateTime::now_utc());
    }
    state.store(STATE_CONNECTED, Ordering::Release);
    tracing::info!(endpoint = %endpoint, "connected to VTube Studio");

    let mut stream = ws;

    loop {
        tokio::select! {
            () = shutdown.notified() => {
                state.store(STATE_DISCONNECTED, Ordering::Release);
                return;
            }
            msg = stream.next() => {
                match msg {
                    None | Some(Err(_)) => {
                        tracing::info!(endpoint = %endpoint, "VTube Studio connection closed");
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
    state.store(STATE_DISCONNECTED, Ordering::Release);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_core::BuiltinStatus;

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
}
