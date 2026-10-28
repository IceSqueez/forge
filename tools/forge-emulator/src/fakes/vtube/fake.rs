use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio_tungstenite::tungstenite::Message;

use super::config::FakeVTubeConfig;
use super::ledger::{TokenCheck, VTubeLedger, VTubePushedEvent, VTubeRequest, VTubeSession};
use super::protocol::{
    self, API_NAME, API_VERSION, ERROR_API_NAME_INVALID, ERROR_API_VERSION_INVALID,
    ERROR_DEVELOPER_NAME_INVALID, ERROR_EVENT_TYPE_UNKNOWN, ERROR_JSON_INVALID,
    ERROR_PLUGIN_DEVELOPER_MISSING, ERROR_PLUGIN_NAME_INVALID, ERROR_PLUGIN_NAME_MISSING,
    ERROR_REQUEST_ID_INVALID, ERROR_REQUEST_TYPE_MISSING, ERROR_REQUIRES_AUTHENTICATION,
    ERROR_TOKEN_MISSING, ERROR_TOKEN_REQUEST_DENIED, SUBSCRIBABLE_EVENTS, VTUBE_STUDIO_VERSION,
};
use super::studio::{Answer, Pushed, Refusal, Studio};
use crate::EmulatorError;
use crate::fixture::Fixture;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const LISTEN_TIMEOUT: Duration = Duration::from_secs(5);
const REBIND_ATTEMPTS: u32 = 40;
const REBIND_PAUSE: Duration = Duration::from_millis(25);
const UNKNOWN_REQUEST_ID: &str = "";
const TOKEN_VALID: &str =
    "Token valid. The plugin is authenticated for the duration of this session.";
const TOKEN_INVALID: &str = "Token invalid. The plugin is not authenticated.";
const ACCESS_DENIED: &str = "User has denied API access for your plugin.";

struct Outbox {
    sender: mpsc::UnboundedSender<String>,
    authenticated: bool,
    subscriptions: Vec<String>,
}

struct Inner {
    ledger: VTubeLedger,
    studio: Studio,
    outboxes: HashMap<u64, Outbox>,
    next_session: u64,
}

struct Shared {
    inner: Mutex<Inner>,
    changes: watch::Sender<u64>,
    token: String,
    approve_token_requests: bool,
}

impl Shared {
    fn mutate<T>(&self, change: impl FnOnce(&mut Inner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let result = change(&mut inner);
        drop(inner);
        self.changes.send_modify(|revision| *revision += 1);
        result
    }

    fn read<T>(&self, look: impl FnOnce(&Inner) -> T) -> T {
        look(&self.inner.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

impl Inner {
    fn broadcast(&mut self, pushed: Vec<Pushed>) -> usize {
        let mut delivered_total = 0;
        for event in pushed {
            let frame = protocol::message(
                event.event_name,
                &protocol::generated_id(),
                event.data.clone(),
            );
            let delivered_to = self
                .outboxes
                .values()
                .filter(|outbox| outbox.authenticated)
                .filter(|outbox| {
                    outbox
                        .subscriptions
                        .iter()
                        .any(|name| name == event.event_name)
                })
                .filter(|outbox| outbox.sender.send(frame.clone()).is_ok())
                .count();
            delivered_total += delivered_to;
            self.ledger.events.push(VTubePushedEvent {
                event_name: event.event_name.to_owned(),
                data: event.data,
                delivered_to,
            });
        }
        delivered_total
    }
}

pub struct FakeVTube {
    shared: Arc<Shared>,
    port: u16,
    online: watch::Sender<bool>,
    listening: watch::Receiver<bool>,
    shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl FakeVTube {
    pub async fn start(config: FakeVTubeConfig) -> Result<Self, EmulatorError> {
        if let Some((field, problem)) = config.problems().into_iter().next() {
            return Err(EmulatorError::InvalidFakeConfig {
                reason: format!("{field} {problem}"),
            });
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| EmulatorError::FakeBind {
                reason: e.to_string(),
            })?;
        let address: SocketAddr = listener.local_addr().map_err(|e| EmulatorError::FakeBind {
            reason: e.to_string(),
        })?;
        let (changes, _) = watch::channel(0);
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                ledger: VTubeLedger::default(),
                studio: Studio::new(&config),
                outboxes: HashMap::new(),
                next_session: 1,
            }),
            changes,
            token: config.token.clone(),
            approve_token_requests: config.approve_token_requests,
        });
        let (online, online_rx) = watch::channel(config.online_at_boot);
        let (listening_tx, listening) = watch::channel(false);
        let (shutdown, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(serve(
            config.online_at_boot.then_some(listener),
            address.port(),
            Arc::clone(&shared),
            online_rx,
            listening_tx,
            shutdown_rx,
        ));
        let fake = Self {
            shared,
            port: address.port(),
            online,
            listening,
            shutdown,
            task: Some(task),
        };
        if config.online_at_boot {
            fake.await_listening(true).await?;
        }
        Ok(fake)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn url(&self) -> String {
        format!("ws://{}:{}", Ipv4Addr::LOCALHOST, self.port)
    }

    pub fn addressed(&self, fixture: &Fixture) -> Fixture {
        let mut addressed = fixture.clone();
        if let Some(vtube) = &mut addressed.vtube {
            vtube.port = self.port;
        }
        addressed
    }

    pub fn is_online(&self) -> bool {
        *self.online.borrow()
    }

    pub async fn go_online(&self) -> Result<(), EmulatorError> {
        if self.is_online() {
            return Err(refused("it is already accepting connections"));
        }
        self.online.send_replace(true);
        self.await_listening(true).await
    }

    pub fn trigger_hotkey(&self, hotkey: &str) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.user_triggers_hotkey(hotkey))
    }

    pub fn load_model(&self, model: &str) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.user_loads_model(model))
    }

    pub fn unload_model(&self) -> Result<usize, EmulatorError> {
        self.push(Studio::user_unloads_model)
    }

    pub fn change_model_config(&self) -> Result<usize, EmulatorError> {
        self.push(Studio::user_changes_model_config)
    }

    pub fn set_face_found(&self, face_found: bool) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.tracking_changes(face_found))
    }

    pub fn add_item(&self, file: &str) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.user_adds_item(file))
    }

    pub fn remove_item(&self, file: &str) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.user_removes_item(file))
    }

    pub fn set_expression(&self, file: &str, active: bool) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.user_sets_expression(file, active))
    }

    pub fn current_model(&self) -> Option<String> {
        self.shared
            .read(|inner| inner.studio.current_model().map(str::to_owned))
    }

    pub fn face_found(&self) -> bool {
        self.shared.read(|inner| inner.studio.face_found())
    }

    pub fn expression_active(&self, file: &str) -> Option<bool> {
        self.shared
            .read(|inner| inner.studio.expression_active(file))
    }

    pub fn items_in_scene(&self) -> Vec<String> {
        self.shared.read(|inner| inner.studio.items_in_scene())
    }

    pub fn ledger(&self) -> VTubeLedger {
        self.shared.read(|inner| inner.ledger.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&VTubeLedger) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        let mut changes = self.shared.changes.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(found) = self.shared.read(|inner| probe(&inner.ledger)) {
                return Ok(found);
            }
            match tokio::time::timeout_at(deadline, changes.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    return Err(EmulatorError::WaitTimeout {
                        what: what.to_owned(),
                    });
                }
            }
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(mut task) = self.task.take()
            && tokio::time::timeout(SHUTDOWN_GRACE, &mut task)
                .await
                .is_err()
        {
            task.abort();
        }
    }

    fn push(
        &self,
        change: impl FnOnce(&mut Studio) -> Result<Vec<Pushed>, Refusal>,
    ) -> Result<usize, EmulatorError> {
        self.shared
            .mutate(|inner| {
                let pushed = change(&mut inner.studio)?;
                Ok(inner.broadcast(pushed))
            })
            .map_err(|Refusal(reason)| EmulatorError::FakeVTubeRefused { reason })
    }

    async fn await_listening(&self, wanted: bool) -> Result<(), EmulatorError> {
        let mut listening = self.listening.clone();
        tokio::time::timeout(LISTEN_TIMEOUT, listening.wait_for(|now| *now == wanted))
            .await
            .map_err(|_| EmulatorError::FakeBind {
                reason: format!(
                    "port {} did not {} in time",
                    self.port,
                    if wanted { "open" } else { "close" }
                ),
            })?
            .map(|_| ())
            .map_err(|_| refused("its server task has stopped"))
    }
}

impl Drop for FakeVTube {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn refused(reason: &str) -> EmulatorError {
    EmulatorError::FakeVTubeRefused {
        reason: reason.to_owned(),
    }
}

async fn serve(
    mut listener: Option<TcpListener>,
    port: u16,
    shared: Arc<Shared>,
    mut online: watch::Receiver<bool>,
    listening: watch::Sender<bool>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        let wanted = *online.borrow_and_update();
        if !wanted {
            listener = None;
        } else if listener.is_none() {
            listener = rebind(port).await;
        }
        listening.send_replace(listener.is_some());
        tokio::select! {
            _ = shutdown.changed() => break,
            changed = online.changed() => if changed.is_err() { break },
            accepted = accept(listener.as_ref()) => {
                if let Ok((stream, _)) = accepted {
                    connections.spawn(connection(
                        stream,
                        Arc::clone(&shared),
                        online.clone(),
                        shutdown.clone(),
                    ));
                }
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    connections.shutdown().await;
}

async fn rebind(port: u16) -> Option<TcpListener> {
    for _ in 0..REBIND_ATTEMPTS {
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
            return Some(listener);
        }
        tokio::time::sleep(REBIND_PAUSE).await;
    }
    None
}

async fn accept(listener: Option<&TcpListener>) -> std::io::Result<(TcpStream, SocketAddr)> {
    match listener {
        Some(listener) => listener.accept().await,
        None => std::future::pending().await,
    }
}

struct Session {
    id: u64,
    authenticated: bool,
    outbox: mpsc::UnboundedSender<String>,
}

async fn connection(
    stream: TcpStream,
    shared: Arc<Shared>,
    mut online: watch::Receiver<bool>,
    mut shutdown: watch::Receiver<bool>,
) {
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (outbox, mut queued) = mpsc::unbounded_channel();
    let id = shared.mutate(|inner| {
        let id = inner.next_session;
        inner.next_session += 1;
        inner.ledger.sessions.push(VTubeSession {
            id,
            authentication: TokenCheck::Pending,
            token_requested: false,
            subscriptions: Vec::new(),
            closed: false,
        });
        inner.outboxes.insert(
            id,
            Outbox {
                sender: outbox.clone(),
                authenticated: false,
                subscriptions: Vec::new(),
            },
        );
        id
    });
    let mut session = Session {
        id,
        authenticated: false,
        outbox,
    };
    let mut going_away = false;
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                going_away = true;
                break;
            }
            changed = online.changed() => {
                if changed.is_err() || !*online.borrow() {
                    going_away = true;
                    break;
                }
            }
            Some(frame) = queued.recv() => {
                if socket.send(Message::Text(frame.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.next() => {
                let text = match incoming {
                    Some(Ok(Message::Text(text))) => text.as_str().to_owned(),
                    Some(Ok(Message::Binary(bytes))) => String::from_utf8_lossy(&bytes).into_owned(),
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => continue,
                };
                session.handle(&shared, &text);
            }
        }
    }
    shared.mutate(|inner| {
        inner.outboxes.remove(&id);
        if let Some(record) = inner.ledger.session_mut(id) {
            record.closed = true;
        }
    });
    if going_away {
        let _ = socket.close(None).await;
    }
}

impl Session {
    fn handle(&mut self, shared: &Shared, text: &str) {
        match envelope(text) {
            Ok((request_id, message_type, data)) => {
                self.respond(shared, &request_id, &message_type, data);
            }
            Err(refusal) => {
                let _ = self.outbox.send(refusal);
            }
        }
    }

    fn respond(&mut self, shared: &Shared, request_id: &str, message_type: &str, data: Value) {
        let (answer, pushed) = self.answer(shared, message_type, &data);
        let error_id = answer.error_id();
        let reply = match answer {
            Answer::Done(data) => {
                protocol::message(&protocol::response_type(message_type), request_id, data)
            }
            Answer::Refused { error_id, reason } => {
                protocol::api_error(request_id, error_id, &reason)
            }
        };
        shared.mutate(|inner| {
            inner.ledger.requests.push(VTubeRequest {
                session: self.id,
                message_type: message_type.to_owned(),
                data,
                error_id,
            });
            let _ = self.outbox.send(reply);
            inner.broadcast(pushed);
        });
    }

    fn answer(
        &mut self,
        shared: &Shared,
        message_type: &str,
        data: &Value,
    ) -> (Answer, Vec<Pushed>) {
        match message_type {
            "APIStateRequest" => (
                Answer::Done(json!({
                    "active": true,
                    "vTubeStudioVersion": VTUBE_STUDIO_VERSION,
                    "currentSessionAuthenticated": self.authenticated,
                })),
                Vec::new(),
            ),
            "AuthenticationTokenRequest" => (self.token_request(shared, data), Vec::new()),
            "AuthenticationRequest" => (self.authenticate(shared, data), Vec::new()),
            _ if !self.authenticated => (
                Answer::refused(
                    ERROR_REQUIRES_AUTHENTICATION,
                    "This request requires authentication.",
                ),
                Vec::new(),
            ),
            "EventSubscriptionRequest" => (self.subscribe(shared, data), Vec::new()),
            _ => shared.mutate(|inner| inner.studio.answer(self.id, message_type, data)),
        }
    }

    fn token_request(&self, shared: &Shared, data: &Value) -> Answer {
        shared.mutate(|inner| {
            if let Some(record) = inner.ledger.session_mut(self.id) {
                record.token_requested = true;
            }
        });
        let name = data
            .get("pluginName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !protocol::plugin_name_is_valid(name) {
            return Answer::refused(
                ERROR_PLUGIN_NAME_INVALID,
                "pluginName has to be 3 to 32 characters long.",
            );
        }
        let developer = data
            .get("pluginDeveloper")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !protocol::plugin_name_is_valid(developer) {
            return Answer::refused(
                ERROR_DEVELOPER_NAME_INVALID,
                "pluginDeveloper has to be 3 to 32 characters long.",
            );
        }
        if !shared.approve_token_requests {
            return Answer::refused(ERROR_TOKEN_REQUEST_DENIED, ACCESS_DENIED);
        }
        Answer::Done(json!({ "authenticationToken": shared.token }))
    }

    fn authenticate(&mut self, shared: &Shared, data: &Value) -> Answer {
        let field = |name: &str| {
            data.get(name)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        };
        let Some(token) = field("authenticationToken") else {
            return Answer::refused(ERROR_TOKEN_MISSING, "The authenticationToken is missing.");
        };
        if field("pluginName").is_none() {
            return Answer::refused(ERROR_PLUGIN_NAME_MISSING, "The pluginName is missing.");
        }
        if field("pluginDeveloper").is_none() {
            return Answer::refused(
                ERROR_PLUGIN_DEVELOPER_MISSING,
                "The pluginDeveloper is missing.",
            );
        }
        let accepted = token == shared.token;
        self.authenticated = self.authenticated || accepted;
        let authenticated = self.authenticated;
        shared.mutate(|inner| {
            if let Some(outbox) = inner.outboxes.get_mut(&self.id) {
                outbox.authenticated = authenticated;
            }
            if let Some(record) = inner.ledger.session_mut(self.id) {
                record.authentication = if accepted {
                    TokenCheck::Accepted
                } else {
                    TokenCheck::Rejected
                };
            }
        });
        Answer::Done(json!({
            "authenticated": accepted,
            "reason": if accepted { TOKEN_VALID } else { TOKEN_INVALID },
        }))
    }

    fn subscribe(&self, shared: &Shared, data: &Value) -> Answer {
        let name = data
            .get("eventName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let subscribe = data
            .get("subscribe")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let known = SUBSCRIBABLE_EVENTS.contains(&name);
        if !known && (subscribe || !name.is_empty()) {
            return Answer::refused(
                ERROR_EVENT_TYPE_UNKNOWN,
                format!("The event type `{name}` is unknown."),
            );
        }
        let subscribed = shared.mutate(|inner| {
            let Some(outbox) = inner.outboxes.get_mut(&self.id) else {
                return Vec::new();
            };
            if subscribe {
                if !outbox.subscriptions.iter().any(|known| known == name) {
                    outbox.subscriptions.push(name.to_owned());
                }
            } else if name.is_empty() {
                outbox.subscriptions.clear();
            } else {
                outbox.subscriptions.retain(|known| known != name);
            }
            let subscribed = outbox.subscriptions.clone();
            if let Some(record) = inner.ledger.session_mut(self.id) {
                record.subscriptions = subscribed.clone();
            }
            subscribed
        });
        Answer::Done(json!({
            "subscribedEventCount": subscribed.len(),
            "subscribedEvents": subscribed,
        }))
    }
}

fn envelope(text: &str) -> Result<(String, String, Value), String> {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return Err(protocol::api_error(
            UNKNOWN_REQUEST_ID,
            ERROR_JSON_INVALID,
            "The message is not valid JSON.",
        ));
    };
    let request_id = match message.get("requestID") {
        None | Some(Value::Null) => protocol::generated_id(),
        Some(id) => match id.as_str().filter(|id| protocol::request_id_is_valid(id)) {
            Some(id) => id.to_owned(),
            None => {
                return Err(protocol::api_error(
                    UNKNOWN_REQUEST_ID,
                    ERROR_REQUEST_ID_INVALID,
                    "The requestID has to be 1 to 64 ASCII characters.",
                ));
            }
        },
    };
    if message.get("apiName").and_then(Value::as_str) != Some(API_NAME) {
        return Err(protocol::api_error(
            &request_id,
            ERROR_API_NAME_INVALID,
            "apiName has to be VTubeStudioPublicAPI.",
        ));
    }
    if message.get("apiVersion").and_then(Value::as_str) != Some(API_VERSION) {
        return Err(protocol::api_error(
            &request_id,
            ERROR_API_VERSION_INVALID,
            "apiVersion has to be 1.0.",
        ));
    }
    let Some(message_type) = message
        .get("messageType")
        .and_then(Value::as_str)
        .filter(|kind| !kind.is_empty())
    else {
        return Err(protocol::api_error(
            &request_id,
            ERROR_REQUEST_TYPE_MISSING,
            "The messageType field is missing or empty.",
        ));
    };
    let data = message
        .get("data")
        .filter(|data| data.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok((request_id, message_type.to_owned(), data))
}
