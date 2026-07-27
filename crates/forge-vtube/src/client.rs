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
use crate::health::{HealthSnapshot, make_health_channel, spawn_health_task};
use crate::protocol::new_request;
use crate::request::PendingRequest;

pub(crate) type ReqTxSlot = Arc<tokio::sync::Mutex<mpsc::UnboundedSender<PendingRequest>>>;

pub(crate) type VtsWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(crate) const DEFAULT_VTS_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_VTS_PORT: u16 = 8001;

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
    pub(crate) api_call_tx: mpsc::UnboundedSender<()>,
    health_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    pub(crate) content_state: Arc<RwLock<crate::content::ContentSnapshot>>,
    pub(crate) content_notifier: crate::content::ContentNotifier,
    content_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
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
        let (api_call_tx, api_call_rx) = mpsc::unbounded_channel::<()>();
        let content_state = Arc::new(RwLock::new(crate::content::ContentSnapshot::default()));
        let (content_notifier, content_changed_rx) = crate::content::ContentNotifier::new();
        let auto_reconnect = Arc::new(AtomicBool::new(true));

        let health_handle = spawn_health_task(
            Arc::clone(&health_state),
            health_tx.clone(),
            req_tx.clone(),
            api_call_rx,
            Arc::clone(&state),
        );
        let content_handle = crate::content::spawn_content_task(
            Arc::clone(&content_state),
            req_tx.clone(),
            content_changed_rx,
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
            auto_reconnect: Arc::clone(&auto_reconnect),
        };
        let handle = tokio::spawn(crate::supervisor::run_supervisor(ctx));

        Self {
            config: cfg,
            vtube_id: BuiltinId::new("vtube"),
            state,
            auth_state,
            shutdown,
            supervisor: Arc::new(std::sync::Mutex::new(Some(handle))),
            connected_at,
            vtube_version,
            req_tx: Arc::new(tokio::sync::Mutex::new(req_tx)),
            health_state,
            health_tx,
            api_call_tx,
            health_task: Arc::new(std::sync::Mutex::new(Some(health_handle))),
            content_state,
            content_notifier,
            content_task: Arc::new(std::sync::Mutex::new(Some(content_handle))),
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
        self.api_call_tx.send(()).ok();
        rx.await.map_err(|_| VTubeError::NotConnected)
    }

    pub async fn shutdown(&self) {
        for task in [&self.health_task, &self.content_task] {
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
        let (api_call_tx, _) = mpsc::unbounded_channel::<()>();
        let publisher = tests::MockPublisher::new().publisher();
        let creds = tests::MockCreds::new().creds();
        Self {
            config: VTubeConfig {
                endpoint: endpoint.into(),
            },
            vtube_id: BuiltinId::new("vtube"),
            state: Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected)),
            auth_state: Arc::new(RwLock::new(AuthState::Cold)),
            shutdown: Arc::new(tokio::sync::Mutex::new(Arc::new(Notify::new()))),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            vtube_version: Arc::new(OnceLock::new()),
            req_tx: Arc::new(tokio::sync::Mutex::new(req_tx)),
            health_state,
            health_tx,
            api_call_tx,
            health_task: Arc::new(std::sync::Mutex::new(None)),
            content_state: Arc::new(RwLock::new(crate::content::ContentSnapshot::default())),
            content_notifier: crate::content::ContentNotifier::noop(),
            content_task: Arc::new(std::sync::Mutex::new(None)),
            auto_reconnect: Arc::new(AtomicBool::new(true)),
            reconnect_publisher: publisher,
            reconnect_creds: creds,
        }
    }
}

impl Drop for VTubeClient {
    fn drop(&mut self) {
        for task in [&self.health_task, &self.content_task] {
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
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicU32, Ordering as AO};
    use std::time::Duration;

    use async_trait::async_trait;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};

    use futures_util::SinkExt;

    use super::*;
    use forge_events::{Event, EventPublisher};

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
                "data": { "authenticationToken": "test-token-abc", "granted": true }
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
            creds.has_key("vtube:default"),
            "token should have been persisted to creds"
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

    #[tokio::test]
    async fn rejected_token_emits_auth_required_and_stops_reconnect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            use tokio_tungstenite::tungstenite::Message;
            let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(
                Duration::from_secs(3),
                futures_util::StreamExt::next(&mut ws),
            )
            .await
            else {
                return;
            };
            let req: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let request_id = req["requestID"].as_str().unwrap_or("unknown");
            let resp = serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": request_id,
                "messageType": "AuthenticationResponse",
                "data": { "authenticated": false, "reason": "Plugin removed" }
            });
            ws.send(Message::Text(resp.to_string().into())).await.ok();
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let token_blob = serde_json::json!({ "token": "stale-token", "api_version": "1.0" });
        creds.insert("vtube:default", &token_blob.to_string());

        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for(|| publisher.disconnected_with_reason("auth_required")).await,
            "expected disconnected event with auth_required reason"
        );
        assert!(
            wait_for(|| !creds.has_key("vtube:default")).await,
            "stale token should have been cleared from creds"
        );
    }

    #[tokio::test]
    async fn denied_popup_emits_auth_denied_reason_distinct_from_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            use tokio_tungstenite::tungstenite::Message;
            let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(
                Duration::from_secs(3),
                futures_util::StreamExt::next(&mut ws),
            )
            .await
            else {
                return;
            };
            let req: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let request_id = req["requestID"].as_str().unwrap_or("unknown");
            let resp = serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": request_id,
                "messageType": "AuthenticationTokenResponse",
                "data": { "authenticationToken": "", "granted": false }
            });
            ws.send(Message::Text(resp.to_string().into())).await.ok();
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        let cfg = VTubeConfig {
            endpoint: format!("ws://{addr}"),
        };
        let _client = VTubeClient::connect(cfg, publisher.publisher(), creds.creds());

        assert!(
            wait_for(|| publisher.disconnected_with_reason("auth_denied")).await,
            "denied popup must map to the auth_denied reason token"
        );
        assert!(
            !publisher.disconnected_with_reason("auth_timeout"),
            "TokenDenied must not collapse into the auth_timeout token"
        );
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
}
