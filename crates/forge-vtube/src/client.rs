use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast, mpsc};
use tokio::task::JoinHandle;

use forge_events::EventPublisher;
use forge_platform_core::{AtomicConnectionState, BuiltinId, ConnectionState, HealthDelta};
use forge_storage::CredentialsRepo;

use crate::auth::AuthState;
use crate::error::VTubeError;
use crate::health::{HealthSnapshot, make_health_channel};
use crate::protocol::new_request;
use crate::request::{PendingRequest, REQUEST_TIMEOUT, ReqTxSlot};

pub(crate) type VtsWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(crate) const DEFAULT_VTS_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_VTS_PORT: u16 = 8001;
pub(crate) const VTUBE_PLATFORM_ID: &str = "vtube";

#[derive(Debug, Clone)]
pub struct VTubeConfig {
    pub endpoint: String,
}

impl Default for VTubeConfig {
    fn default() -> Self {
        Self {
            endpoint: format!("ws://{DEFAULT_VTS_HOST}:{DEFAULT_VTS_PORT}/"),
        }
    }
}

pub(crate) fn split_endpoint(endpoint: &str) -> (String, u16) {
    let without_scheme = endpoint
        .strip_prefix("ws://")
        .or_else(|| endpoint.strip_prefix("wss://"))
        .unwrap_or(endpoint);

    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);

    match authority.rsplit_once(':') {
        Some((host, port_str)) => match port_str.parse::<u16>() {
            Ok(port) => (host.to_owned(), port),
            Err(_) => (authority.to_owned(), DEFAULT_VTS_PORT),
        },
        None => (authority.to_owned(), DEFAULT_VTS_PORT),
    }
}

pub struct VTubeClient {
    pub(crate) config: VTubeConfig,
    pub(crate) vtube_id: BuiltinId,
    pub(crate) state: Arc<AtomicConnectionState>,
    pub(crate) auth_state: Arc<RwLock<AuthState>>,
    // async Mutex: reconnect swaps the Notify without racing the supervisor's own clone.
    pub(crate) shutdown: Arc<tokio::sync::Mutex<Arc<Notify>>>,
    pub(crate) supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    pub(crate) connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    pub(crate) vtube_version: Arc<OnceLock<String>>,
    pub(crate) req_tx: ReqTxSlot,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) content_state: Arc<RwLock<crate::content::ContentSnapshot>>,
    pub(crate) content_notifier: crate::content::ContentNotifier,
    pub(crate) connected_notifier: mpsc::UnboundedSender<()>,
    content_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    catalog_metrics_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    version_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    pub(crate) auto_reconnect: Arc<AtomicBool>,
    // Never logged or surfaced.
    pub(crate) reconnect_publisher: Arc<dyn EventPublisher>,
    pub(crate) reconnect_creds: Arc<dyn CredentialsRepo>,
}

impl VTubeClient {
    pub fn connect(
        cfg: VTubeConfig,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Self {
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connecting));
        let auth_state = Arc::new(RwLock::new(AuthState::Cold));
        let notify = Arc::new(Notify::new());
        let shutdown = Arc::new(tokio::sync::Mutex::new(Arc::clone(&notify)));
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));
        let vtube_version = Arc::new(OnceLock::<String>::new());
        let (health_tx, health_state) = make_health_channel();
        let (req_tx, req_rx) = mpsc::unbounded_channel::<PendingRequest>();
        let req_tx_slot: ReqTxSlot = Arc::new(tokio::sync::Mutex::new(req_tx.clone()));
        let content_state = Arc::new(RwLock::new(crate::content::ContentSnapshot::default()));
        let (content_notifier, content_changed_rx) = crate::content::ContentNotifier::new();
        let (connected_tx, connected_rx) = mpsc::unbounded_channel::<()>();
        let auto_reconnect = Arc::new(AtomicBool::new(true));

        let content_handle = crate::content::spawn_content_task(
            Arc::clone(&content_state),
            Arc::clone(&req_tx_slot),
            content_changed_rx,
        );
        let catalog_metrics_handle = crate::content::spawn_catalog_metrics_task(
            Arc::clone(&content_state),
            Arc::clone(&health_state),
            Arc::clone(&req_tx_slot),
            health_tx.clone(),
        );
        let version_handle = crate::content::spawn_version_fetch(
            Arc::clone(&req_tx_slot),
            Arc::clone(&vtube_version),
            connected_rx,
        );

        let ctx = crate::supervisor::SupervisorContext {
            endpoint: cfg.endpoint.clone(),
            state: Arc::clone(&state),
            auth_state: Arc::clone(&auth_state),
            shutdown: Arc::clone(&notify),
            connected_at: Arc::clone(&connected_at),
            publisher: Arc::clone(&publisher),
            creds: Arc::clone(&creds),
            req_rx,
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            content_notifier: content_notifier.clone(),
            connected_notifier: connected_tx.clone(),
            auto_reconnect: Arc::clone(&auto_reconnect),
        };
        let handle = tokio::spawn(crate::supervisor::run_supervisor(ctx));

        Self {
            config: cfg,
            vtube_id: BuiltinId::new(VTUBE_PLATFORM_ID),
            state,
            auth_state,
            shutdown,
            supervisor: Arc::new(std::sync::Mutex::new(Some(handle))),
            connected_at,
            vtube_version,
            req_tx: req_tx_slot,
            health_state,
            health_tx,
            content_state,
            content_notifier,
            connected_notifier: connected_tx,
            content_task: Arc::new(std::sync::Mutex::new(Some(content_handle))),
            catalog_metrics_task: Arc::new(std::sync::Mutex::new(Some(catalog_metrics_handle))),
            version_task: Arc::new(std::sync::Mutex::new(Some(version_handle))),
            auto_reconnect,
            reconnect_publisher: publisher,
            reconnect_creds: creds,
        }
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state.load()
    }

    pub fn auth_state(&self) -> AuthState {
        self.auth_state.read().map_or(AuthState::Cold, |g| *g)
    }

    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.auto_reconnect.store(enabled, Ordering::Relaxed);
    }

    pub fn auto_reconnect_enabled(&self) -> bool {
        self.auto_reconnect.load(Ordering::Relaxed)
    }

    pub(crate) async fn send_json_request(
        &self,
        msg_type: &str,
        data: serde_json::Value,
    ) -> Result<serde_json::Value, VTubeError> {
        if !self.state.load().is_connected() {
            return Err(VTubeError::NotConnected);
        }
        let req = new_request(msg_type, data);
        let request_id = req.request_id.clone();
        let payload = serde_json::to_string(&req).map_err(VTubeError::Json)?;
        let (respond_to, rx) = tokio::sync::oneshot::channel();
        // Lock is held only for the synchronous .send() - not across any .await.
        {
            let tx = self.req_tx.lock().await;
            tx.send(PendingRequest {
                request_id,
                payload,
                respond_to,
            })
            .map_err(|_| VTubeError::NotConnected)?;
        }
        tokio::time::timeout(REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| VTubeError::Timeout)?
            .map_err(|_| VTubeError::NotConnected)
    }

    pub async fn shutdown(&self) {
        for task in [
            &self.content_task,
            &self.catalog_metrics_task,
            &self.version_task,
        ] {
            if let Some(h) = task.lock().ok().and_then(|mut g| g.take()) {
                h.abort();
            }
        }
        let notify = self.shutdown.lock().await.clone();
        notify.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(endpoint: impl Into<String>) -> Self {
        let (health_tx, health_state) = make_health_channel();
        let (req_tx, _) = mpsc::unbounded_channel::<PendingRequest>();
        let (connected_notifier, _) = mpsc::unbounded_channel::<()>();
        let publisher = tests::MockPublisher::new().publisher();
        let creds = tests::MockCreds::new().creds();
        Self {
            config: VTubeConfig {
                endpoint: endpoint.into(),
            },
            vtube_id: BuiltinId::new(VTUBE_PLATFORM_ID),
            state: Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected)),
            auth_state: Arc::new(RwLock::new(AuthState::Cold)),
            shutdown: Arc::new(tokio::sync::Mutex::new(Arc::new(Notify::new()))),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            vtube_version: Arc::new(OnceLock::new()),
            req_tx: Arc::new(tokio::sync::Mutex::new(req_tx)),
            health_state,
            health_tx,
            content_state: Arc::new(RwLock::new(crate::content::ContentSnapshot::default())),
            content_notifier: crate::content::ContentNotifier::noop(),
            connected_notifier,
            content_task: Arc::new(std::sync::Mutex::new(None)),
            catalog_metrics_task: Arc::new(std::sync::Mutex::new(None)),
            version_task: Arc::new(std::sync::Mutex::new(None)),
            auto_reconnect: Arc::new(AtomicBool::new(true)),
            reconnect_publisher: publisher,
            reconnect_creds: creds,
        }
    }
}

impl Drop for VTubeClient {
    fn drop(&mut self) {
        for task in [
            &self.content_task,
            &self.catalog_metrics_task,
            &self.version_task,
        ] {
            if let Ok(mut g) = task.lock()
                && let Some(h) = g.take()
            {
                h.abort();
            }
        }
        if let Ok(notify) = self.shutdown.try_lock() {
            notify.notify_one();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicU32, Ordering as AO};
    use std::time::Duration;

    use async_trait::async_trait;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};

    use futures_util::SinkExt;

    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::BuiltinControl;

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
                        && e.payload["is_connected"].as_bool() == Some(true)
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
                        && e.payload["is_connected"].as_bool() == Some(false)
                })
                .cloned()
        }

        pub(crate) fn disconnected_with_reason(&self, reason: &str) -> bool {
            self.events.lock().unwrap().iter().any(|e| {
                e.kind == "vtube.connection.changed"
                    && e.payload["is_connected"].as_bool() == Some(false)
                    && e.payload["reason"].as_str() == Some(reason)
            })
        }
    }

    impl EventPublisher for MockPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    pub(crate) struct MockCreds {
        store: Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
    }

    impl MockCreds {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                store: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            })
        }

        pub(crate) fn creds(self: &Arc<Self>) -> Arc<dyn CredentialsRepo> {
            Arc::clone(self) as Arc<dyn CredentialsRepo>
        }

        pub(crate) fn has_key(&self, key: &str) -> bool {
            self.store.lock().unwrap().contains_key(key)
        }

        pub(crate) fn value(&self, key: &str) -> Option<String> {
            self.store.lock().unwrap().get(key).cloned()
        }

        pub(crate) fn insert(&self, key: &str, value: &str) {
            self.store
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
        }
    }

    #[async_trait]
    impl CredentialsRepo for MockCreds {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.store
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.store.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.store.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .keys()
                .map(|k| CredentialId::new(k.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _id: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    pub(crate) async fn serve_full_auth(
        ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) {
        use tokio_tungstenite::tungstenite::Message;
        let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(Duration::from_secs(3), futures_util::StreamExt::next(ws)).await
        else {
            return;
        };
        let req: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let request_id = req["requestID"].as_str().unwrap_or("unknown");
        let msg_type = req["messageType"].as_str().unwrap_or("");

        if msg_type == "AuthenticationTokenRequest" {
            let resp = serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": request_id,
                "messageType": "AuthenticationTokenResponse",
                "data": { "authenticationToken": "test-token-abc" }
            });
            ws.send(Message::Text(resp.to_string().into())).await.ok();

            let Ok(Some(Ok(Message::Text(text2)))) =
                tokio::time::timeout(Duration::from_secs(3), futures_util::StreamExt::next(ws))
                    .await
            else {
                return;
            };
            let req2: serde_json::Value = serde_json::from_str(&text2).unwrap_or_default();
            let rid2 = req2["requestID"].as_str().unwrap_or("unknown");
            let auth_resp = serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": rid2,
                "messageType": "AuthenticationResponse",
                "data": { "authenticated": true, "reason": "" }
            });
            ws.send(Message::Text(auth_resp.to_string().into()))
                .await
                .ok();
        } else if msg_type == "AuthenticationRequest" {
            let auth_resp = serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": request_id,
                "messageType": "AuthenticationResponse",
                "data": { "authenticated": true, "reason": "" }
            });
            ws.send(Message::Text(auth_resp.to_string().into()))
                .await
                .ok();
        }
    }

    pub(crate) async fn wait_for(cond: impl Fn() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    pub(crate) async fn wait_for_connected(publisher: &MockPublisher) -> bool {
        wait_for(|| publisher.connected_event().is_some()).await
    }

    /// Short enough that a paused clock creeps forward instead of leaping to the next
    /// supervisor deadline while a reply is still sitting in the loopback socket.
    pub(crate) const PAUSED_STEP: Duration = Duration::from_millis(10);

    /// Waits on a paused clock in `PAUSED_STEP` increments; returns false once `budget` elapses.
    pub(crate) async fn wait_paused(budget: Duration, cond: impl Fn() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + budget;
        while tokio::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            tokio::time::sleep(PAUSED_STEP).await;
        }
        cond()
    }

    const CLOCK_FREEZE_REAL_LIMIT: Duration = Duration::from_secs(30);

    /// While alive, the paused clock cannot auto-advance: tokio holds time still as long as a
    /// blocking task runs. Frames crossing the loopback socket then arrive at the paused instant
    /// they were sent, even when a loaded kernel delivers them late in real time.
    pub(crate) struct ClockFreeze {
        _release: std::sync::mpsc::Sender<()>,
    }

    pub(crate) fn freeze_clock() -> ClockFreeze {
        let (release, released) = std::sync::mpsc::channel::<()>();
        drop(tokio::task::spawn_blocking(move || {
            let _ = released.recv_timeout(CLOCK_FREEZE_REAL_LIMIT);
        }));
        ClockFreeze { _release: release }
    }

    pub(crate) fn stored_token_creds() -> Arc<MockCreds> {
        let creds = MockCreds::new();
        creds.insert(
            "vtube:default",
            &serde_json::json!({ "token": "stored-token", "api_version": "1.0" }).to_string(),
        );
        creds
    }

    enum PeerCmd {
        Send(serde_json::Value),
        Close,
    }

    #[derive(Clone)]
    pub(crate) struct PeerTx(mpsc::UnboundedSender<PeerCmd>);

    impl PeerTx {
        fn frame(message_type: &str, request_id: &str, data: serde_json::Value) -> PeerCmd {
            PeerCmd::Send(serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": request_id,
                "messageType": message_type,
                "data": data,
            }))
        }

        pub(crate) fn reply(&self, request: &serde_json::Value, data: serde_json::Value) {
            let request_type = request["messageType"].as_str().unwrap_or("");
            let response_type = request_type.replace("Request", "Response");
            let request_id = request["requestID"].as_str().unwrap_or("");
            let _ = self.0.send(Self::frame(&response_type, request_id, data));
        }

        pub(crate) fn api_error(&self, request: &serde_json::Value, error_id: i64, message: &str) {
            let request_id = request["requestID"].as_str().unwrap_or("");
            let _ = self.0.send(Self::frame(
                "APIError",
                request_id,
                serde_json::json!({ "errorID": error_id, "message": message }),
            ));
        }

        pub(crate) fn event(&self, message_type: &str, data: serde_json::Value) {
            let _ = self.0.send(Self::frame(message_type, "", data));
        }

        pub(crate) fn close(&self) {
            let _ = self.0.send(PeerCmd::Close);
        }
    }

    pub(crate) struct PeerConn {
        frames: mpsc::UnboundedReceiver<serde_json::Value>,
        backlog: std::collections::VecDeque<serde_json::Value>,
        pub(crate) tx: PeerTx,
    }

    impl PeerConn {
        /// Next frame from forge whose `messageType` is `message_type`; frames of other types
        /// are kept for later calls. Panics when none arrives within `budget` of paused time.
        pub(crate) async fn expect(
            &mut self,
            message_type: &str,
            budget: Duration,
        ) -> serde_json::Value {
            if let Some(pos) = self
                .backlog
                .iter()
                .position(|f| f["messageType"] == message_type)
            {
                return self.backlog.remove(pos).unwrap();
            }
            let deadline = tokio::time::Instant::now() + budget;
            while tokio::time::Instant::now() < deadline {
                match tokio::time::timeout(PAUSED_STEP, self.frames.recv()).await {
                    Ok(Some(frame)) if frame["messageType"] == message_type => return frame,
                    Ok(Some(frame)) => self.backlog.push_back(frame),
                    Ok(None) => panic!("the connection closed while waiting for {message_type}"),
                    Err(_) => {}
                }
            }
            panic!("no {message_type} from forge within {budget:?}");
        }

        pub(crate) async fn next_frame(&mut self) -> Option<serde_json::Value> {
            match self.backlog.pop_front() {
                Some(frame) => Some(frame),
                None => self.frames.recv().await,
            }
        }

        pub(crate) async fn accept_login(&mut self) {
            let login = self
                .expect("AuthenticationRequest", Duration::from_secs(5))
                .await;
            self.tx.reply(
                &login,
                serde_json::json!({ "authenticated": true, "reason": "" }),
            );
        }

        /// Hands the frame stream to a task that answers every request with an empty success
        /// body, except the frames `is_silent` picks, which it counts and leaves unanswered.
        pub(crate) fn answer_all_except(
            mut self,
            is_silent: fn(&serde_json::Value) -> bool,
        ) -> (PeerTx, Arc<AtomicU32>) {
            let tx = self.tx.clone();
            let ignored = Arc::new(AtomicU32::new(0));
            let counter = Arc::clone(&ignored);
            let responder = self.tx.clone();
            tokio::spawn(async move {
                let mut pending: Vec<_> = self.backlog.drain(..).collect();
                loop {
                    for frame in pending.drain(..) {
                        if is_silent(&frame) {
                            counter.fetch_add(1, AO::SeqCst);
                        } else {
                            responder.reply(&frame, serde_json::json!({}));
                        }
                    }
                    match self.frames.recv().await {
                        Some(frame) => pending.push(frame),
                        None => return,
                    }
                }
            });
            (tx, ignored)
        }
    }

    /// Scripted VTube Studio: every accepted connection is handed to the test as a `PeerConn`.
    pub(crate) struct FakeVts {
        pub(crate) endpoint: String,
        conns: mpsc::UnboundedReceiver<PeerConn>,
    }

    impl FakeVts {
        pub(crate) async fn bind() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("ws://{}", listener.local_addr().unwrap());
            let (conns_tx, conns) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    if stream.set_nodelay(true).is_err() {
                        continue;
                    }
                    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                        continue;
                    };
                    let (frames_tx, frames) = mpsc::unbounded_channel();
                    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
                    tokio::spawn(pump_peer_connection(ws, frames_tx, cmd_rx));
                    let conn = PeerConn {
                        frames,
                        backlog: std::collections::VecDeque::new(),
                        tx: PeerTx(cmd_tx),
                    };
                    if conns_tx.send(conn).is_err() {
                        return;
                    }
                }
            });
            Self { endpoint, conns }
        }

        pub(crate) fn connect(
            &self,
            publisher: &Arc<MockPublisher>,
            creds: &Arc<MockCreds>,
        ) -> VTubeClient {
            VTubeClient::connect(
                VTubeConfig {
                    endpoint: self.endpoint.clone(),
                },
                publisher.publisher(),
                creds.creds(),
            )
        }

        pub(crate) async fn next_conn(&mut self, budget: Duration) -> Option<PeerConn> {
            let deadline = tokio::time::Instant::now() + budget;
            while tokio::time::Instant::now() < deadline {
                if let Ok(conn) = tokio::time::timeout(PAUSED_STEP, self.conns.recv()).await {
                    return conn;
                }
            }
            None
        }

        pub(crate) async fn logged_in_conn(&mut self) -> PeerConn {
            let Some(mut conn) = self.next_conn(Duration::from_secs(5)).await else {
                panic!("forge never dialed the fake VTube Studio");
            };
            conn.accept_login().await;
            conn
        }
    }

    async fn pump_peer_connection(
        mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        frames: mpsc::UnboundedSender<serde_json::Value>,
        mut cmds: mpsc::UnboundedReceiver<PeerCmd>,
    ) {
        use tokio_tungstenite::tungstenite::Message;
        loop {
            tokio::select! {
                frame = futures_util::StreamExt::next(&mut ws) => match frame {
                    Some(Ok(Message::Text(text))) => {
                        let _ = frames.send(serde_json::from_str(&text).unwrap_or_default());
                        // Why: forge's socket leaves Nagle on, so its next frame waits for our
                        // ACK; an unsolicited pong carries that ACK at once instead of after the
                        // delayed-ACK timer, which real time never reaches on a paused clock.
                        if ws.send(Message::Pong(Vec::new().into())).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(_)) => {}
                    _ => return,
                },
                cmd = cmds.recv() => match cmd {
                    Some(PeerCmd::Send(value)) => {
                        if ws.send(Message::Text(value.to_string().into())).await.is_err() {
                            return;
                        }
                    }
                    Some(PeerCmd::Close) | None => {
                        let _ = ws.close(None).await;
                        return;
                    }
                },
            }
        }
    }

    #[tokio::test]
    async fn connect_emits_connected_event_when_server_accepts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve_full_auth(&mut ws).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for_connected(&publisher).await,
            "expected connected event"
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
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve_full_auth(&mut ws).await;
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for(|| publisher.disconnected_event().is_some()).await,
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
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                serve_full_auth(&mut ws).await;
            }
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        let result = tokio::time::timeout(Duration::from_secs(5), async {
            accept_rx.recv().await;
            accept_rx.recv().await;
        })
        .await;

        assert!(result.is_ok(), "expected reconnect within 5 s");
        assert!(accept_count.load(AO::Acquire) >= 2);
    }

    #[tokio::test]
    async fn cold_start_auth_stores_token_and_reaches_connected() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve_full_auth(&mut ws).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for_connected(&publisher).await,
            "expected connected after cold-start auth"
        );
        assert!(
            creds
                .value("vtube:default")
                .is_some_and(|blob| blob.contains("test-token-abc")),
            "the token from the documented reply must be persisted"
        );
    }

    #[tokio::test]
    async fn stored_token_skips_token_request_and_reaches_connected() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve_full_auth(&mut ws).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let token_blob = serde_json::json!({ "token": "pre-stored-tok", "api_version": "1.0" });
        creds.insert("vtube:default", &token_blob.to_string());

        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for_connected(&publisher).await,
            "expected connected with stored token"
        );
    }

    fn first_disconnect_reason(publisher: &MockPublisher) -> Option<String> {
        publisher
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "vtube.connection.changed")
            .filter_map(|e| e.payload["reason"].as_str())
            .find(|reason| *reason != "awaiting_approval")
            .map(str::to_owned)
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_login_asks_for_auth_again_and_forgets_the_stored_token() {
        type Rejection = fn(&PeerTx, &serde_json::Value);
        let rejections: [(&str, Rejection); 2] = [
            ("authenticated false", |tx, login| {
                tx.reply(
                    login,
                    serde_json::json!({ "authenticated": false, "reason": "Plugin removed" }),
                );
            }),
            ("APIError 50", |tx, login| {
                tx.api_error(login, 50, "User has denied API access for this plugin");
            }),
        ];
        for (shape, reject) in rejections {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let creds = stored_token_creds();
            let _client = vts.connect(&publisher, &creds);
            let mut conn = vts.next_conn(Duration::from_secs(5)).await.unwrap();

            let login = conn
                .expect("AuthenticationRequest", Duration::from_secs(5))
                .await;
            reject(&conn.tx, &login);

            assert!(
                wait_paused(Duration::from_secs(5), || {
                    publisher.disconnected_with_reason("auth_required")
                })
                .await,
                "{shape}: expected the auth_required reason"
            );
            assert!(
                !creds.has_key("vtube:default"),
                "{shape}: the rejected token must be cleared from the store"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_pairing_popup_reply_decides_between_denied_and_a_retryable_failure() {
        type PopupReply = fn(&PeerTx, &serde_json::Value);
        let cases: [(&str, PopupReply, &str); 3] = [
            (
                "user clicked Deny (APIError 50)",
                |tx, req| tx.api_error(req, 50, "User has denied API access for this plugin"),
                "auth_denied",
            ),
            (
                "any other APIError",
                |tx, req| tx.api_error(req, 1, "Internal server error"),
                "auth_failed",
            ),
            (
                "token response without a token",
                |tx, req| tx.reply(req, serde_json::json!({ "authenticationToken": "" })),
                "auth_failed",
            ),
        ];
        for (case, answer, expected) in cases {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let _client = vts.connect(&publisher, &MockCreds::new());
            let mut conn = vts.next_conn(Duration::from_secs(5)).await.unwrap();

            let popup = conn
                .expect("AuthenticationTokenRequest", Duration::from_secs(5))
                .await;
            answer(&conn.tx, &popup);

            assert!(
                wait_paused(Duration::from_secs(5), || {
                    first_disconnect_reason(&publisher).is_some()
                })
                .await,
                "{case}: no outcome was reported"
            );
            assert_eq!(
                first_disconnect_reason(&publisher).as_deref(),
                Some(expected),
                "{case}"
            );
        }
    }

    #[tokio::test]
    async fn connected_client_polls_expression_state() {
        let (seen_tx, mut seen_rx) = tokio::sync::mpsc::channel::<()>(1);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve_full_auth(&mut ws).await;
            use tokio_tungstenite::tungstenite::Message;
            while let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(
                Duration::from_secs(5),
                futures_util::StreamExt::next(&mut ws),
            )
            .await
            {
                let req: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                if req["messageType"] == "ExpressionStateRequest" {
                    let _ = seen_tx.send(()).await;
                    break;
                }
            }
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        let seen = tokio::time::timeout(Duration::from_secs(5), seen_rx.recv()).await;
        assert!(
            matches!(seen, Ok(Some(()))),
            "a connected client must poll ExpressionStateRequest"
        );
    }

    #[tokio::test]
    async fn connect_failure_emits_connect_failed_reason_with_detail() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for(|| publisher.disconnected_with_reason("connect_failed")).await,
            "a refused connection must emit the connect_failed reason token"
        );
        let ev = publisher
            .events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.payload["reason"].as_str() == Some("connect_failed"))
            .cloned()
            .unwrap();
        assert!(
            ev.payload["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "connect_failed must carry a non-empty human detail string"
        );
    }

    #[test]
    fn split_endpoint_recovers_the_host_and_port_that_get_persisted() {
        for (endpoint, expected) in [
            ("ws://127.0.0.1:8001/", ("127.0.0.1", 8001)),
            ("wss://vts.local:9123", ("vts.local", 9123)),
            ("ws://vts.local/", ("vts.local", 8001)),
            ("ws://[::1]:8001/", ("[::1]", 8001)),
        ] {
            let (host, port) = split_endpoint(endpoint);

            assert_eq!((host.as_str(), port), expected, "endpoint {endpoint}");
        }
    }

    /// Reads the connection state at the instant each event is published, which is the only
    /// vantage point that can tell "stored, then published" apart from "published, then stored".
    struct StateProbePublisher {
        client: Arc<OnceLock<Arc<VTubeClient>>>,
        tx: mpsc::UnboundedSender<(Event, Option<ConnectionState>)>,
    }

    impl EventPublisher for StateProbePublisher {
        fn publish(&self, event: Event) {
            let state = self.client.get().map(|c| c.connection_state());
            let _ = self.tx.send((event, state));
        }
    }

    type ProbedEvents = mpsc::UnboundedReceiver<(Event, Option<ConnectionState>)>;

    async fn state_when(
        rx: &mut ProbedEvents,
        matches: impl Fn(&Event) -> bool,
    ) -> Option<Option<ConnectionState>> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (event, state) = rx.recv().await?;
                if matches(&event) {
                    return Some(state);
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    fn is_connected(event: &Event) -> bool {
        event.payload["is_connected"].as_bool() == Some(true)
    }

    fn has_reason(event: &Event, reason: &str) -> bool {
        event.payload["reason"].as_str() == Some(reason)
    }

    /// Holds an authenticated connection open until `gate` fires, so a test can finish its own
    /// setup before the peer drops the socket under the supervisor.
    async fn serve_auth_then_gated_close(
        listener: tokio::net::TcpListener,
        gate: tokio::sync::oneshot::Receiver<()>,
    ) {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        serve_full_auth(&mut ws).await;
        if gate.await.is_err() {
            return;
        }
        let _ = ws.close(None).await;
    }

    /// Rejects the stored token, but only once `gate` fires.
    async fn serve_gated_token_rejection(
        listener: tokio::net::TcpListener,
        gate: tokio::sync::oneshot::Receiver<()>,
    ) {
        use tokio_tungstenite::tungstenite::Message;
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(
            Duration::from_secs(3),
            futures_util::StreamExt::next(&mut ws),
        )
        .await
        else {
            return;
        };
        let req: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let request_id = req["requestID"].as_str().unwrap_or("unknown").to_owned();
        if gate.await.is_err() {
            return;
        }
        let resp = serde_json::json!({
            "apiName": "VTubeStudioPublicAPI",
            "apiVersion": "1.0",
            "requestID": request_id,
            "messageType": "AuthenticationResponse",
            "data": { "authenticated": false, "reason": "Plugin removed" }
        });
        ws.send(Message::Text(resp.to_string().into())).await.ok();
        while let Some(Ok(_)) = futures_util::StreamExt::next(&mut ws).await {}
    }

    // Why: the VTube screen reloads off vtube.connection.changed and then reads the connection
    // state back. Publishing before the state was stored let that read observe the state the
    // connection was leaving, so the header kept claiming a live connection after it dropped.
    #[tokio::test]
    async fn the_connection_state_is_already_settled_when_a_socket_close_is_published() {
        for (auto_reconnect, expected) in [
            (false, ConnectionState::Disconnected),
            (true, ConnectionState::Reconnecting),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(serve_auth_then_gated_close(listener, gate_rx));
            let (tx, mut rx) = mpsc::unbounded_channel();
            let slot: Arc<OnceLock<Arc<VTubeClient>>> = Arc::new(OnceLock::new());

            let client = Arc::new(VTubeClient::connect(
                VTubeConfig {
                    endpoint: format!("ws://{addr}"),
                },
                Arc::new(StateProbePublisher {
                    client: Arc::clone(&slot),
                    tx,
                }),
                MockCreds::new().creds(),
            ));
            let _ = slot.set(Arc::clone(&client));
            assert!(
                state_when(&mut rx, is_connected).await.is_some(),
                "expected the client to reach connected before the peer drops the socket"
            );
            client.set_auto_reconnect(auto_reconnect);
            let _ = gate_tx.send(());

            let state = state_when(&mut rx, |e| has_reason(e, "socket_closed")).await;

            server.abort();
            drop(client);
            assert_eq!(
                state,
                Some(Some(expected)),
                "state observed at the socket_closed publish with auto_reconnect = {auto_reconnect}"
            );
        }
    }

    #[tokio::test]
    async fn a_closed_socket_stops_the_supervisor_when_auto_reconnect_is_off() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_auth_then_gated_close(listener, gate_rx));
        let publisher = MockPublisher::new();
        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            publisher.publisher(),
            MockCreds::new().creds(),
        );
        assert!(wait_for_connected(&publisher).await, "expected connected");

        client.set_auto_reconnect(false);
        let _ = gate_tx.send(());

        let handle = client.supervisor.lock().unwrap().take().unwrap();
        let stopped = tokio::time::timeout(Duration::from_secs(5), handle).await;
        server.abort();
        assert!(
            stopped.is_ok(),
            "the supervisor kept looping after the socket closed with auto-reconnect off"
        );
    }

    // Why: a rejected token cannot be fixed by dialing again, so the terminal auth exits must
    // ignore the retry flag entirely rather than fall into the backoff loop.
    #[tokio::test]
    async fn a_rejected_token_stops_the_supervisor_even_with_auto_reconnect_on() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_gated_token_rejection(listener, gate_rx));
        let creds = MockCreds::new();
        creds.insert(
            "vtube:default",
            &serde_json::json!({ "token": "stale-token", "api_version": "1.0" }).to_string(),
        );
        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            MockPublisher::new().publisher(),
            creds.creds(),
        );

        client.set_auto_reconnect(true);
        let _ = gate_tx.send(());

        let handle = client.supervisor.lock().unwrap().take().unwrap();
        let stopped = tokio::time::timeout(Duration::from_secs(5), handle).await;
        server.abort();
        assert!(
            stopped.is_ok(),
            "a rejected token must end the supervisor even while retries are enabled"
        );
    }

    #[tokio::test]
    async fn the_connection_state_is_already_disconnected_when_the_auth_rejection_is_published() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_gated_token_rejection(listener, gate_rx));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let slot: Arc<OnceLock<Arc<VTubeClient>>> = Arc::new(OnceLock::new());
        let creds = MockCreds::new();
        creds.insert(
            "vtube:default",
            &serde_json::json!({ "token": "stale-token", "api_version": "1.0" }).to_string(),
        );

        let client = Arc::new(VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            Arc::new(StateProbePublisher {
                client: Arc::clone(&slot),
                tx,
            }),
            creds.creds(),
        ));
        let _ = slot.set(Arc::clone(&client));
        let _ = gate_tx.send(());

        let state = state_when(&mut rx, |e| has_reason(e, "auth_required")).await;

        server.abort();
        drop(client);
        assert_eq!(state, Some(Some(ConnectionState::Disconnected)));
    }

    // Why: the approval popup blocks inside VTube Studio with no feedback on our side. Without
    // this phase the screen sits on "connecting" for the whole 30 s token wait.
    #[tokio::test]
    async fn the_client_announces_that_it_is_waiting_for_the_vts_approval_popup() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            while let Some(Ok(_)) = futures_util::StreamExt::next(&mut ws).await {}
        });
        let publisher = MockPublisher::new();

        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            publisher.publisher(),
            MockCreds::new().creds(),
        );

        assert!(
            wait_for(|| publisher.disconnected_with_reason("awaiting_approval")).await,
            "a cold start must announce the pending approval popup"
        );
        assert_eq!(client.auth_state(), AuthState::AwaitingApproval);
    }

    const UNREACHABLE_VTS: &str = "ws://192.0.2.1:8001/";

    const PROMPT_DISCONNECT: Duration = Duration::from_secs(2);

    const RETRY_BUDGET: Duration = Duration::from_secs(600);

    const ATTEMPTS_BEFORE_VERDICT: usize = 3;

    const PEER_SPEAKS_WINDOW: Duration = Duration::from_secs(2);

    async fn serve_refused_handshakes(
        listener: tokio::net::TcpListener,
        accepted: mpsc::UnboundedSender<()>,
    ) {
        while let Ok((stream, _)) = listener.accept().await {
            drop(stream);
            if accepted.send(()).is_err() {
                return;
            }
        }
    }

    async fn serve_auth_then_announced_close(
        listener: tokio::net::TcpListener,
        gate: tokio::sync::oneshot::Receiver<()>,
        settled: tokio::sync::oneshot::Sender<()>,
    ) {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        serve_full_auth(&mut ws).await;
        if gate.await.is_err() {
            return;
        }
        let _ = ws.close(None).await;
        let _ = settled.send(());
        while let Some(Ok(_)) = futures_util::StreamExt::next(&mut ws).await {}
    }

    fn announced_states(publisher: &MockPublisher) -> Vec<String> {
        publisher
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == forge_platform_core::CONNECTION_STATE_CHANGED_KIND)
            .map(|e| {
                e.payload["state"]
                    .as_str()
                    .unwrap_or("<missing>")
                    .to_owned()
            })
            .collect()
    }

    #[tokio::test]
    async fn a_backoff_loop_announces_one_state_change_per_real_transition() {
        tokio::time::pause();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (accept_tx, mut accept_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(serve_refused_handshakes(listener, accept_tx));
        let publisher = MockPublisher::new();

        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            publisher.publisher(),
            MockCreds::new().creds(),
        );

        for attempt in 1..=ATTEMPTS_BEFORE_VERDICT {
            tokio::time::timeout(RETRY_BUDGET, accept_rx.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!("the supervisor stopped retrying before attempt {attempt}")
                })
                .unwrap_or_else(|| panic!("the mock server stopped before attempt {attempt}"));
        }
        client.disconnect().await.unwrap();
        server.abort();

        assert_eq!(
            announced_states(&publisher),
            vec!["reconnecting".to_owned(), "disconnected".to_owned()],
            "{ATTEMPTS_BEFORE_VERDICT} refused attempts must announce one retry, then the shutdown"
        );
    }

    #[tokio::test]
    async fn a_session_reports_each_step_on_the_vtube_feed_as_well_as_the_shared_one() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
        let (settled_tx, _settled_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_auth_then_announced_close(
            listener, gate_rx, settled_tx,
        ));
        let publisher = MockPublisher::new();

        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            publisher.publisher(),
            MockCreds::new().creds(),
        );
        assert!(wait_for_connected(&publisher).await, "expected connected");
        assert_eq!(
            announced_states(&publisher),
            vec!["connected".to_owned()],
            "reaching a live session must also announce it on the shared feed"
        );

        let _ = gate_tx.send(());
        assert!(
            wait_for(|| publisher.disconnected_with_reason("socket_closed")).await,
            "a dropped socket must still name socket_closed on the vtube feed"
        );

        let states = announced_states(&publisher);
        server.abort();
        drop(client);
        assert_eq!(
            states,
            vec!["connected".to_owned(), "reconnecting".to_owned()],
            "the shared feed must follow the same session steps the vtube feed reports"
        );
    }

    #[tokio::test]
    async fn disconnecting_during_an_unreachable_connect_does_not_wait_out_the_tcp_timeout() {
        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: UNREACHABLE_VTS.to_owned(),
            },
            MockPublisher::new().publisher(),
            MockCreds::new().creds(),
        );
        tokio::task::yield_now().await;

        let outcome = tokio::time::timeout(PROMPT_DISCONNECT, client.disconnect()).await;

        assert!(
            outcome.is_ok(),
            "disconnect queued behind the connect instead of cancelling it"
        );
    }

    #[tokio::test]
    async fn a_retired_client_stays_silent_when_its_server_speaks_afterwards() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
        let (settled_tx, settled_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_auth_then_announced_close(
            listener, gate_rx, settled_tx,
        ));
        let publisher = MockPublisher::new();

        let client = VTubeClient::connect(
            VTubeConfig {
                endpoint: format!("ws://{addr}"),
            },
            publisher.publisher(),
            MockCreds::new().creds(),
        );
        assert!(wait_for_connected(&publisher).await, "expected connected");
        client.disconnect().await.unwrap();
        publisher.events.lock().unwrap().clear();

        let _ = gate_tx.send(());
        let _ = tokio::time::timeout(PEER_SPEAKS_WINDOW, settled_rx).await;
        tokio::task::yield_now().await;
        server.abort();

        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "a client whose disconnect already returned published again"
        );
    }
}
