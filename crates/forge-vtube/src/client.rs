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
            reconnect_publisher: publisher,
            reconnect_creds: creds,
        }
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state.load()
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

    pub(crate) async fn wait_for_connected(publisher: &MockPublisher) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if publisher.connected_event().is_some() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
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

        tokio::time::sleep(Duration::from_millis(500)).await;
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

        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(
            publisher.disconnected_with_reason("auth_required"),
            "expected disconnected event with auth_required reason"
        );
        assert!(
            !creds.has_key("vtube:default"),
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

        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(
            publisher.disconnected_with_reason("auth_denied"),
            "denied popup must map to the auth_denied reason token"
        );
        assert!(
            !publisher.disconnected_with_reason("auth_timeout"),
            "TokenDenied must not collapse into the auth_timeout token"
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

        tokio::time::sleep(Duration::from_millis(300)).await;

        assert!(
            publisher.disconnected_with_reason("connect_failed"),
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
}
