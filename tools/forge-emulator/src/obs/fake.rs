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
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

use super::config::FakeObsConfig;
use super::ledger::{Authentication, ObsLedger, ObsPushedEvent, ObsRequest, ObsSession};
use super::protocol::{
    self, CLOSE_ALREADY_IDENTIFIED, CLOSE_AUTHENTICATION_FAILED, CLOSE_GOING_AWAY,
    CLOSE_MESSAGE_DECODE_ERROR, CLOSE_MISSING_DATA_FIELD, CLOSE_NOT_IDENTIFIED,
    CLOSE_UNKNOWN_OP_CODE, CLOSE_UNSUPPORTED_RPC_VERSION, OP_IDENTIFY, OP_REIDENTIFY, OP_REQUEST,
    RPC_VERSION, SUBSCRIBE_ALL,
};
use super::studio::{Pushed, Refusal, Studio};
use crate::EmulatorError;
use crate::fixture::Fixture;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const LISTEN_TIMEOUT: Duration = Duration::from_secs(5);
const REBIND_ATTEMPTS: u32 = 40;
const REBIND_PAUSE: Duration = Duration::from_millis(25);

struct Outbox {
    sender: mpsc::UnboundedSender<String>,
    subscriptions: u64,
}

struct Inner {
    ledger: ObsLedger,
    studio: Studio,
    outboxes: HashMap<u64, Outbox>,
    next_session: u64,
}

struct Shared {
    inner: Mutex<Inner>,
    changes: watch::Sender<u64>,
    password: Option<String>,
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
            let frame = protocol::event(event.event_type, event.intent, &event.data);
            let delivered_to = self
                .outboxes
                .values()
                .filter(|outbox| outbox.subscriptions & event.intent != 0)
                .filter(|outbox| outbox.sender.send(frame.clone()).is_ok())
                .count();
            delivered_total += delivered_to;
            self.ledger.events.push(ObsPushedEvent {
                event_type: event.event_type.to_owned(),
                data: event.data,
                delivered_to,
            });
        }
        delivered_total
    }
}

pub struct FakeObs {
    shared: Arc<Shared>,
    port: u16,
    online: watch::Sender<bool>,
    listening: watch::Receiver<bool>,
    shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl FakeObs {
    pub async fn start(config: FakeObsConfig) -> Result<Self, EmulatorError> {
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
                ledger: ObsLedger::default(),
                studio: Studio::new(&config),
                outboxes: HashMap::new(),
                next_session: 1,
            }),
            changes,
            password: config.password.clone(),
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
        if let Some(obs) = &mut addressed.obs {
            obs.port = self.port;
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

    pub async fn restart(&self, down_for: Duration) -> Result<usize, EmulatorError> {
        if !self.is_online() {
            return Err(refused("it is not running, so there is nothing to restart"));
        }
        let live = self.ledger().live_sessions().count();
        self.online.send_replace(false);
        self.await_listening(false).await?;
        tokio::time::sleep(down_for).await;
        self.online.send_replace(true);
        self.await_listening(true).await?;
        Ok(live)
    }

    pub fn switch_scene(&self, scene: &str) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.switch_scene(scene))
    }

    pub fn set_streaming(&self, active: bool) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.set_streaming(active))
    }

    pub fn set_input_mute(&self, input: &str, muted: bool) -> Result<usize, EmulatorError> {
        self.push(|studio| studio.set_input_mute(input, muted))
    }

    pub fn current_scene(&self) -> Option<String> {
        self.shared
            .read(|inner| inner.studio.current_scene().map(str::to_owned))
    }

    pub fn streaming(&self) -> bool {
        self.shared.read(|inner| inner.studio.streaming())
    }

    pub fn input_muted(&self, input: &str) -> Option<bool> {
        self.shared.read(|inner| inner.studio.input_muted(input))
    }

    pub fn ledger(&self) -> ObsLedger {
        self.shared.read(|inner| inner.ledger.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&ObsLedger) -> Option<T>,
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
            .map_err(|Refusal(reason)| EmulatorError::FakeObsRefused { reason })
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

impl Drop for FakeObs {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn refused(reason: &str) -> EmulatorError {
    EmulatorError::FakeObsRefused {
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

enum Flow {
    Reply(Vec<String>),
    Close(u16),
}

struct Session {
    id: u64,
    identified: bool,
    challenge: Option<(String, String)>,
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
    let challenge = shared
        .password
        .as_ref()
        .map(|_| (protocol::random_token(), protocol::random_token()));
    let id = shared.mutate(|inner| {
        let id = inner.next_session;
        inner.next_session += 1;
        inner.ledger.sessions.push(ObsSession {
            id,
            authentication: if challenge.is_some() {
                Authentication::Pending
            } else {
                Authentication::NotRequired
            },
            identified: false,
            event_subscriptions: None,
            closed: false,
            close_code: None,
        });
        id
    });
    let mut session = Session {
        id,
        identified: false,
        challenge,
        outbox,
    };
    let hello = protocol::hello(
        session
            .challenge
            .as_ref()
            .map(|(challenge, salt)| (challenge.as_str(), salt.as_str())),
    );
    let mut close = None;
    if socket.send(Message::Text(hello.into())).await.is_ok() {
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    close = Some(CLOSE_GOING_AWAY);
                    break;
                }
                changed = online.changed() => {
                    if changed.is_err() || !*online.borrow() {
                        close = Some(CLOSE_GOING_AWAY);
                        break;
                    }
                }
                Some(frame) = queued.recv() => {
                    if socket.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                }
                incoming = socket.next() => match incoming {
                    Some(Ok(Message::Text(text))) => match session.handle(&shared, text.as_str()) {
                        Flow::Reply(frames) => {
                            let mut sent = true;
                            for frame in frames {
                                if socket.send(Message::Text(frame.into())).await.is_err() {
                                    sent = false;
                                    break;
                                }
                            }
                            if !sent {
                                break;
                            }
                        }
                        Flow::Close(code) => {
                            close = Some(code);
                            break;
                        }
                    },
                    Some(Ok(Message::Binary(_))) => {
                        close = Some(CLOSE_MESSAGE_DECODE_ERROR);
                        break;
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
    shared.mutate(|inner| {
        inner.outboxes.remove(&id);
        if let Some(record) = inner.ledger.session_mut(id) {
            record.closed = true;
            record.close_code = close;
        }
    });
    let frame = close.map(|code| CloseFrame {
        code: CloseCode::from(code),
        reason: close_reason(code).into(),
    });
    let _ = socket.close(frame).await;
}

fn close_reason(code: u16) -> &'static str {
    match code {
        CLOSE_GOING_AWAY => "Server stopping.",
        CLOSE_MESSAGE_DECODE_ERROR => "Unable to decode the message.",
        CLOSE_MISSING_DATA_FIELD => "Your message is missing a required field.",
        CLOSE_UNKNOWN_OP_CODE => "Unknown OpCode.",
        CLOSE_NOT_IDENTIFIED => {
            "You attempted to send a non-Identify message while not identified."
        }
        CLOSE_ALREADY_IDENTIFIED => "You are already Identified with the obs-websocket server.",
        CLOSE_AUTHENTICATION_FAILED => "Authentication failed.",
        CLOSE_UNSUPPORTED_RPC_VERSION => "Your requested RPC version is not supported.",
        _ => "",
    }
}

impl Session {
    fn handle(&mut self, shared: &Shared, text: &str) -> Flow {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            return Flow::Close(CLOSE_MESSAGE_DECODE_ERROR);
        };
        let (Some(op), Some(data)) = (
            message.get("op").and_then(Value::as_u64),
            message.get("d").filter(|data| data.is_object()),
        ) else {
            return Flow::Close(CLOSE_MISSING_DATA_FIELD);
        };
        if !self.identified {
            return if op == OP_IDENTIFY {
                self.identify(shared, data)
            } else {
                Flow::Close(CLOSE_NOT_IDENTIFIED)
            };
        }
        match op {
            OP_IDENTIFY => Flow::Close(CLOSE_ALREADY_IDENTIFIED),
            OP_REIDENTIFY => {
                let subscriptions = subscriptions_of(data);
                shared.mutate(|inner| {
                    if let Some(outbox) = inner.outboxes.get_mut(&self.id) {
                        outbox.subscriptions = subscriptions;
                    }
                    if let Some(record) = inner.ledger.session_mut(self.id) {
                        record.event_subscriptions = Some(subscriptions);
                    }
                });
                Flow::Reply(vec![protocol::identified()])
            }
            OP_REQUEST => self.request(shared, data),
            _ => Flow::Close(CLOSE_UNKNOWN_OP_CODE),
        }
    }

    fn identify(&mut self, shared: &Shared, data: &Value) -> Flow {
        let Some(rpc_version) = data.get("rpcVersion").and_then(Value::as_u64) else {
            return Flow::Close(CLOSE_MISSING_DATA_FIELD);
        };
        if rpc_version != RPC_VERSION {
            return Flow::Close(CLOSE_UNSUPPORTED_RPC_VERSION);
        }
        let authentication = match (&self.challenge, &shared.password) {
            (Some((challenge, salt)), Some(password)) => {
                let expected = protocol::authentication_string(password, salt, challenge);
                if data.get("authentication").and_then(Value::as_str) == Some(expected.as_str()) {
                    Authentication::Accepted
                } else {
                    Authentication::Rejected
                }
            }
            _ => Authentication::NotRequired,
        };
        let subscriptions = subscriptions_of(data);
        let accepted = authentication != Authentication::Rejected;
        shared.mutate(|inner| {
            if let Some(record) = inner.ledger.session_mut(self.id) {
                record.authentication = authentication;
                record.identified = accepted;
                record.event_subscriptions = accepted.then_some(subscriptions);
            }
            if accepted {
                inner.outboxes.insert(
                    self.id,
                    Outbox {
                        sender: self.outbox.clone(),
                        subscriptions,
                    },
                );
            }
        });
        if !accepted {
            return Flow::Close(CLOSE_AUTHENTICATION_FAILED);
        }
        self.identified = true;
        Flow::Reply(vec![protocol::identified()])
    }

    fn request(&self, shared: &Shared, data: &Value) -> Flow {
        let (Some(request_type), Some(request_id)) = (
            data.get("requestType").and_then(Value::as_str),
            data.get("requestId").and_then(Value::as_str),
        ) else {
            return Flow::Close(CLOSE_MISSING_DATA_FIELD);
        };
        let request_data = data
            .get("requestData")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let response = shared.mutate(|inner| {
            let (answer, pushed) = inner.studio.answer(request_type, &request_data);
            inner.ledger.requests.push(ObsRequest {
                session: self.id,
                request_type: request_type.to_owned(),
                request_data,
                code: answer.code,
            });
            inner.broadcast(pushed);
            protocol::response(request_type, request_id, &answer)
        });
        Flow::Reply(vec![response])
    }
}

fn subscriptions_of(data: &Value) -> u64 {
    data.get("eventSubscriptions")
        .and_then(Value::as_u64)
        .unwrap_or(SUBSCRIBE_ALL)
}
