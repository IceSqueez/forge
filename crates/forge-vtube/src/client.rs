use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use forge_events::{Event, EventPublisher, EventSource};
use forge_platform_core::{BuiltinId, ConnectionState, HealthDelta};
use forge_storage::CredentialsRepo;

use crate::auth::AuthState;
use crate::error::VTubeError;
use crate::health::{HealthSnapshot, make_health_channel, spawn_health_task, update_from_event};
use crate::protocol::new_request;
use crate::request::PendingRequest;

pub(crate) const STATE_DISCONNECTED: u8 = 0;
const STATE_CONNECTING: u8 = 1;
pub(crate) const STATE_CONNECTED: u8 = 2;
const STATE_RECONNECTING: u8 = 3;

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
    pub(crate) state: Arc<AtomicU8>,
    auth_state: Arc<RwLock<AuthState>>,
    shutdown: Arc<Notify>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    pub(crate) connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    pub(crate) vtube_version: Arc<OnceLock<String>>,
    pub(crate) req_tx: mpsc::UnboundedSender<PendingRequest>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) api_call_tx: mpsc::UnboundedSender<()>,
    health_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    pub(crate) content_state: Arc<RwLock<crate::content::ContentSnapshot>>,
    pub(crate) content_notifier: crate::content::ContentNotifier,
    content_task: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
}

impl VTubeClient {
    pub fn connect(
        cfg: VTubeConfig,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Self {
        let state = Arc::new(AtomicU8::new(STATE_CONNECTING));
        let auth_state = Arc::new(RwLock::new(AuthState::Cold));
        let shutdown = Arc::new(Notify::new());
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

        let ctx = SupervisorContext {
            endpoint: cfg.endpoint.clone(),
            state: Arc::clone(&state),
            auth_state: Arc::clone(&auth_state),
            shutdown: Arc::clone(&shutdown),
            connected_at: Arc::clone(&connected_at),
            publisher,
            creds,
            req_rx,
            health_state: Arc::clone(&health_state),
            health_tx: health_tx.clone(),
            content_notifier: content_notifier.clone(),
        };
        let handle = tokio::spawn(run_supervisor(ctx));

        Self {
            config: cfg,
            vtube_id: BuiltinId::new("vtube"),
            state,
            auth_state,
            shutdown,
            supervisor: Arc::new(std::sync::Mutex::new(Some(handle))),
            connected_at,
            vtube_version,
            req_tx,
            health_state,
            health_tx,
            api_call_tx,
            health_task: Arc::new(std::sync::Mutex::new(Some(health_handle))),
            content_state,
            content_notifier,
            content_task: Arc::new(std::sync::Mutex::new(Some(content_handle))),
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

    pub fn auth_state_value(&self) -> AuthState {
        self.auth_state.read().ok().map_or(AuthState::Cold, |g| *g)
    }

    pub(crate) async fn send_json_request(
        &self,
        msg_type: &str,
        data: serde_json::Value,
    ) -> Result<serde_json::Value, VTubeError> {
        if self.state.load(Ordering::Acquire) != STATE_CONNECTED {
            return Err(VTubeError::NotConnected);
        }
        let req = new_request(msg_type, data);
        let request_id = req.request_id.clone();
        let payload = serde_json::to_string(&req).map_err(VTubeError::Json)?;
        let (respond_to, rx) = tokio::sync::oneshot::channel();
        self.req_tx
            .send(PendingRequest {
                request_id,
                payload,
                respond_to,
            })
            .map_err(|_| VTubeError::NotConnected)?;
        self.api_call_tx.send(()).ok();
        rx.await.map_err(|_| VTubeError::NotConnected)
    }

    pub async fn shutdown(&self) {
        for task in [&self.health_task, &self.content_task] {
            if let Some(h) = task.lock().ok().and_then(|mut g| g.take()) {
                h.abort();
            }
        }
        self.shutdown.notify_one();
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
        Self {
            config: VTubeConfig {
                endpoint: endpoint.into(),
            },
            vtube_id: BuiltinId::new("vtube"),
            state: Arc::new(AtomicU8::new(STATE_DISCONNECTED)),
            auth_state: Arc::new(RwLock::new(AuthState::Cold)),
            shutdown: Arc::new(Notify::new()),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
            vtube_version: Arc::new(OnceLock::new()),
            req_tx,
            health_state,
            health_tx,
            api_call_tx,
            health_task: Arc::new(std::sync::Mutex::new(None)),
            content_state: Arc::new(RwLock::new(crate::content::ContentSnapshot::default())),
            content_notifier: crate::content::ContentNotifier::noop(),
            content_task: Arc::new(std::sync::Mutex::new(None)),
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
        self.shutdown.notify_one();
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

async fn send_ws_msg<T: serde::Serialize>(ws: &mut VtsWs, msg: &T) -> Result<(), VTubeError> {
    let text = serde_json::to_string(msg).map_err(VTubeError::Json)?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| VTubeError::Connect(e.to_string()))
}

async fn recv_next_text(ws: &mut VtsWs) -> Result<serde_json::Value, VTubeError> {
    loop {
        match ws.next().await {
            None => return Err(VTubeError::Connect("connection closed".to_owned())),
            Some(Err(e)) => return Err(VTubeError::Connect(e.to_string())),
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(VTubeError::Json);
            }
            Some(Ok(_)) => {}
        }
    }
}

async fn request_new_token(ws: &mut VtsWs, endpoint: &str) -> Result<String, VTubeError> {
    let req = new_request(
        "AuthenticationTokenRequest",
        serde_json::json!({ "pluginName": "forge", "pluginDeveloper": "forge" }),
    );
    send_ws_msg(ws, &req).await?;
    tracing::debug!(endpoint, "sent AuthenticationTokenRequest, awaiting popup");

    let msg = tokio::time::timeout(Duration::from_secs(30), recv_next_text(ws))
        .await
        .map_err(|_| VTubeError::TokenTimeout)??;

    let msg_type = msg["messageType"].as_str().unwrap_or("");
    if msg_type != "AuthenticationTokenResponse" {
        return Err(VTubeError::Request {
            message: format!("expected AuthenticationTokenResponse, got {msg_type}"),
        });
    }

    let granted = msg["data"]["granted"].as_bool().unwrap_or(false);
    if !granted {
        return Err(VTubeError::TokenDenied);
    }

    msg["data"]["authenticationToken"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| VTubeError::Request {
            message: "authenticationToken missing in response".to_owned(),
        })
}

async fn authenticate_with_token(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    token: &str,
    endpoint: &str,
) -> Result<(), VTubeError> {
    let req = new_request(
        "AuthenticationRequest",
        serde_json::json!({
            "pluginName": "forge",
            "pluginDeveloper": "forge",
            "authenticationToken": token
        }),
    );
    send_ws_msg(ws, &req).await?;
    tracing::debug!(endpoint, "sent AuthenticationRequest");

    let msg = recv_next_text(ws).await?;

    let msg_type = msg["messageType"].as_str().unwrap_or("");
    if msg_type != "AuthenticationResponse" {
        return Err(VTubeError::Request {
            message: format!("expected AuthenticationResponse, got {msg_type}"),
        });
    }

    let authenticated = msg["data"]["authenticated"].as_bool().unwrap_or(false);
    if !authenticated {
        let _ = crate::credentials::clear(creds).await;
        tracing::warn!(
            endpoint,
            "VTube Studio token rejected; cleared stored credential"
        );
        return Err(VTubeError::TokenRejected);
    }

    Ok(())
}

async fn run_auth(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    endpoint: &str,
) -> Result<(), VTubeError> {
    let stored = crate::credentials::load(creds).await.ok().flatten();

    let token = if let Some(c) = stored {
        tracing::debug!(endpoint, "using stored VTube Studio credential");
        c.token
    } else {
        let new_token = request_new_token(ws, endpoint).await?;
        if let Err(e) = crate::credentials::store(creds, &new_token, "1.0").await {
            tracing::warn!(endpoint, error = %e, "failed to persist VTube Studio token");
        }
        new_token
    };

    authenticate_with_token(ws, creds, &token, endpoint).await
}

struct SupervisorContext {
    endpoint: String,
    state: Arc<AtomicU8>,
    auth_state: Arc<RwLock<AuthState>>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    publisher: Arc<dyn EventPublisher>,
    creds: Arc<dyn CredentialsRepo>,
    req_rx: mpsc::UnboundedReceiver<PendingRequest>,
    health_state: Arc<RwLock<HealthSnapshot>>,
    health_tx: broadcast::Sender<HealthDelta>,
    content_notifier: crate::content::ContentNotifier,
}

async fn run_supervisor(ctx: SupervisorContext) {
    let SupervisorContext {
        endpoint,
        state,
        auth_state,
        shutdown,
        connected_at,
        publisher,
        creds,
        mut req_rx,
        health_state,
        health_tx,
        content_notifier,
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

        let mut ws = match tokio_tungstenite::connect_async(&endpoint).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    attempt,
                    error = %e,
                    "VTube Studio connection attempt failed"
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
                attempt = attempt.saturating_add(1);
                continue;
            }
        };

        match run_auth(&mut ws, &*creds, &endpoint).await {
            Ok(()) => {}
            Err(VTubeError::TokenRejected) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some("auth_required".to_owned()),
                );
                state.store(STATE_DISCONNECTED, Ordering::Release);
                return;
            }
            Err(VTubeError::TokenDenied | VTubeError::TokenTimeout) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some("auth_denied".to_owned()),
                );
                state.store(STATE_DISCONNECTED, Ordering::Release);
                return;
            }
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "auth failed, will retry"
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
                attempt = attempt.saturating_add(1);
                continue;
            }
        }

        if let Err(e) = crate::events::subscribe_all(&mut ws).await {
            tracing::debug!(endpoint = %endpoint, error = %e, "event subscription failed, will retry");
            emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
            attempt = attempt.saturating_add(1);
            continue;
        }

        if let Ok(mut g) = connected_at.write() {
            *g = Some(OffsetDateTime::now_utc());
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Connected;
        }
        state.store(STATE_CONNECTED, Ordering::Release);
        emit_connection_changed(&*publisher, &endpoint, true, None);
        tracing::info!(endpoint = %endpoint, "connected and authenticated to VTube Studio");

        let mut pending: HashMap<String, tokio::sync::oneshot::Sender<serde_json::Value>> =
            HashMap::new();

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    state.store(STATE_DISCONNECTED, Ordering::Release);
                    if let Ok(mut g) = auth_state.write() {
                        *g = AuthState::Cold;
                    }
                    emit_connection_changed(&*publisher, &endpoint, false, None);
                    return;
                }
                Some(req) = req_rx.recv() => {
                    if ws
                        .send(Message::Text(req.payload.into()))
                        .await
                        .is_ok()
                    {
                        pending.insert(req.request_id, req.respond_to);
                    }
                }
                msg = ws.next() => {
                    match msg {
                        None | Some(Err(_)) => {
                            tracing::info!(endpoint = %endpoint, "VTube Studio connection closed");
                            pending.clear();
                            break;
                        }
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(val) =
                                serde_json::from_str::<serde_json::Value>(&text)
                            {
                                let msg_type = val["messageType"].as_str().unwrap_or("");
                                if msg_type.ends_with("Response") {
                                    let req_id =
                                        val["requestID"].as_str().unwrap_or("").to_owned();
                                    if let Some(tx) = pending.remove(&req_id) {
                                        let _ = tx.send(val["data"].clone());
                                    }
                                } else if let Ok(env) = serde_json::from_value::<
                                    crate::events::RawEnvelope,
                                >(val)
                                {
                                    if env.message_type == "ModelLoadedEvent" {
                                        content_notifier.notify_model_changed();
                                    }
                                    crate::events::dispatch_vts_event(&env, &*publisher);
                                    update_from_event(&env, &health_state, &health_tx);
                                }
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        if let Ok(mut g) = connected_at.write() {
            *g = None;
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Cold;
        }
        emit_connection_changed(&*publisher, &endpoint, false, None);
        attempt = 1;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicU32, Ordering as AO};

    use async_trait::async_trait;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};

    use super::*;
    use forge_events::EventPublisher;

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

        pub(crate) fn disconnected_with_reason(&self, reason: &str) -> bool {
            self.events.lock().unwrap().iter().any(|e| {
                e.kind == "vtube.connection.changed"
                    && e.payload["connected"].as_bool() == Some(false)
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

    async fn serve_full_auth(ws: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) {
        use tokio_tungstenite::tungstenite::Message;
        // Read the first request from client
        let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(Duration::from_secs(3), futures_util::StreamExt::next(ws)).await
        else {
            return;
        };
        let req: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let request_id = req["requestID"].as_str().unwrap_or("unknown");
        let msg_type = req["messageType"].as_str().unwrap_or("");

        // If it's a token request, respond with token then accept auth
        if msg_type == "AuthenticationTokenRequest" {
            let resp = serde_json::json!({
                "apiName": "VTubeStudioPublicAPI",
                "apiVersion": "1.0",
                "requestID": request_id,
                "messageType": "AuthenticationTokenResponse",
                "data": { "authenticationToken": "test-token-abc", "granted": true }
            });
            ws.send(Message::Text(resp.to_string().into())).await.ok();

            // Read auth request
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
            // Stored-token path: respond with authenticated
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

    async fn wait_for_connected(publisher: &MockPublisher) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if publisher.connected_event().is_some() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
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
            // drop ws to close connection
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
                // drop ws
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

    // ── auth-specific tests ───────────────────────────────────────────────────

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
            // serve_full_auth handles the stored-token path (AuthenticationRequest only)
            serve_full_auth(&mut ws).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        });

        let publisher = MockPublisher::new();
        let creds = MockCreds::new();
        // Pre-populate stored token
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
            // Reject any AuthenticationRequest
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
        // Pre-populate stored token that will be rejected
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
}
