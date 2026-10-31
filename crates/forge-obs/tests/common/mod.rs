#![allow(dead_code, clippy::unwrap_used)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::{Event, EventPublisher, PlatformConnectionChanged, TypedEvent};
use forge_obs::{ObsClient, ObsTiming};
use forge_platform_core::ConnectionState;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinSet;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const HELLO_OP: u64 = 0;
const IDENTIFIED_OP: u64 = 2;
const EVENT_OP: u64 = 5;
const RESPONSE_OP: u64 = 7;
const RPC_VERSION: u64 = 1;
const NOT_A_WEB_SOCKET_UPGRADE: &[u8] = b"BAD\r\n\r\n";

pub(crate) struct ChannelPublisher(pub mpsc::UnboundedSender<Event>);

impl EventPublisher for ChannelPublisher {
    fn publish(&self, event: Event) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.0.send(event);
    }
}

pub(crate) fn fast_retries() -> ObsTiming {
    ObsTiming {
        reconnect_base_delay: Duration::from_millis(1),
        reconnect_max_delay: Duration::from_millis(5),
        ..ObsTiming::default()
    }
}

pub(crate) async fn bind_loopback() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

pub(crate) const WALL_BUDGET: Duration = Duration::from_secs(90);

pub(crate) const REAL_CLOCK_POLL: Duration = Duration::from_millis(10);

pub(crate) const SCENE: &str = "Main";
pub(crate) const SCENE_UUID: &str = "6f1d3c1e-8a3b-4b8e-9d1a-0c2b3a4d5e6f";
pub(crate) const SOURCE: &str = "Webcam";
pub(crate) const SNAPSHOT_ITEM_ID: i64 = 7;
pub(crate) const RELOOKED_ITEM_ID: i64 = 9;

pub(crate) const REQUEST_OP: u64 = 6;
pub(crate) const IDENTIFY_OP: u64 = 1;
pub(crate) const OBS_SUCCESS: u16 = 100;
pub(crate) const OBS_RESOURCE_NOT_FOUND: u16 = 600;

pub(crate) async fn settle_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < WALL_BUDGET, "never happened: {what}");
        tokio::time::sleep(REAL_CLOCK_POLL).await;
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Handshake {
    Complete,
    CloseAfterIdentified,
    Stall,
    StallUpgrade,
}

pub(crate) enum Reply {
    Answer(Value),
    Reject,
    RejectWith(u16),
    Silent,
    DropSocket,
    AnswerThenClose(Value),
    EventsFirst(Vec<Value>, Box<Reply>),
    HeldUntil(Arc<Notify>, Box<Reply>),
}

#[derive(Clone, Copy)]
pub(crate) struct Seen<'a> {
    pub conn: usize,
    pub request_type: &'a str,
    pub nth: usize,
}

pub(crate) type Script = Arc<dyn Fn(Seen<'_>) -> Reply + Send + Sync>;

#[derive(Clone, Default)]
pub(crate) struct WireLog(Arc<Mutex<WireState>>);

#[derive(Default)]
pub(crate) struct WireState {
    accepted: usize,
    requests: Vec<(usize, String, Value)>,
}

impl WireLog {
    pub(crate) fn accepted(&self) -> usize {
        self.0.lock().unwrap().accepted
    }

    pub(crate) fn count(&self, conn: usize, request_type: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(c, t, _)| *c == conn && t == request_type)
            .count()
    }

    pub(crate) fn requests_of(&self, conn: usize, types: &[&str]) -> Vec<(String, Value)> {
        self.0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(c, t, _)| *c == conn && types.contains(&t.as_str()))
            .map(|(_, t, d)| (t.clone(), d.clone()))
            .collect()
    }
}

#[derive(Clone, Default)]
pub(crate) struct Recorder(Arc<Mutex<Vec<Event>>>);

impl EventPublisher for Recorder {
    fn publish(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Recorder {
    pub(crate) fn kinds(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind.clone())
            .collect()
    }

    pub(crate) fn announcements(&self) -> Vec<(String, Option<bool>)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == PlatformConnectionChanged::KIND)
            .map(|e| {
                (
                    e.payload["state"]
                        .as_str()
                        .unwrap_or("<missing>")
                        .to_owned(),
                    e.payload["expected"].as_bool(),
                )
            })
            .collect()
    }

    pub(crate) fn states(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == PlatformConnectionChanged::KIND)
            .map(|e| {
                e.payload["state"]
                    .as_str()
                    .unwrap_or("<missing>")
                    .to_owned()
            })
            .collect()
    }
}

pub(crate) fn version_data() -> Value {
    json!({
        "obsVersion": "31.0.0",
        "obsWebSocketVersion": "5.5.0",
        "rpcVersion": RPC_VERSION,
        "availableRequests": [],
        "supportedImageFormats": [],
        "platform": "linux",
        "platformDescription": "loopback fake",
    })
}

pub(crate) fn standard_reply(request_type: &str) -> Reply {
    match request_type {
        "GetVersion" => Reply::Answer(version_data()),
        "GetSceneList" => Reply::Answer(json!({
            "scenes": [{ "sceneName": SCENE, "sceneUuid": SCENE_UUID, "sceneIndex": 0 }],
        })),
        "GetSceneItemList" => Reply::Answer(json!({
            "sceneItems": [{
                "sceneItemId": SNAPSHOT_ITEM_ID,
                "sceneItemIndex": 0,
                "sourceName": SOURCE,
                "sourceType": "OBS_SOURCE_TYPE_INPUT",
                "inputKind": "browser_source",
                "isGroup": null,
            }],
        })),
        "GetSceneItemEnabled" => Reply::Answer(json!({ "sceneItemEnabled": true })),
        "GetSceneItemId" => Reply::Answer(json!({ "sceneItemId": RELOOKED_ITEM_ID })),
        t if t.starts_with("Set") => Reply::Answer(Value::Null),
        _ => Reply::Reject,
    }
}

pub(crate) fn event_frame(event_type: &str, data: Value) -> Value {
    json!({ "op": EVENT_OP, "d": { "eventType": event_type, "eventData": data } })
}

pub(crate) struct FakeObs {
    pub port: u16,
    pub log: WireLog,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for FakeObs {
    fn drop(&mut self) {
        self.server.abort();
    }
}

pub(crate) async fn spawn_fake_obs(handshake: fn(usize) -> Handshake, script: Script) -> FakeObs {
    let (listener, port) = bind_loopback().await;
    let log = WireLog::default();
    let server = tokio::spawn(serve_fake_obs(listener, handshake, script, log.clone()));
    FakeObs { port, log, server }
}

async fn serve_fake_obs(
    listener: TcpListener,
    handshake: fn(usize) -> Handshake,
    script: Script,
    log: WireLog,
) {
    let mut connections = JoinSet::new();
    while let Ok((stream, _)) = listener.accept().await {
        let conn = {
            let mut state = log.0.lock().unwrap();
            state.accepted += 1;
            state.accepted - 1
        };
        connections.spawn(serve_fake_connection(
            stream,
            conn,
            handshake(conn),
            Arc::clone(&script),
            log.clone(),
        ));
    }
}

async fn send_json(socket: &mut WebSocketStream<TcpStream>, frame: &Value) -> bool {
    socket.send(Message::text(frame.to_string())).await.is_ok()
}

async fn close_socket(socket: &mut WebSocketStream<TcpStream>) {
    let frame = CloseFrame {
        code: CloseCode::Normal,
        reason: "fake OBS closing".into(),
    };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = socket.send(Message::Close(Some(frame))).await;
}

async fn next_json(socket: &mut WebSocketStream<TcpStream>) -> Option<Value> {
    while let Some(Ok(message)) = socket.next().await {
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).ok();
        }
    }
    None
}

async fn serve_fake_connection(
    stream: TcpStream,
    conn: usize,
    handshake: Handshake,
    script: Script,
    log: WireLog,
) {
    if matches!(handshake, Handshake::StallUpgrade) {
        std::future::pending::<()>().await;
    }
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    if matches!(handshake, Handshake::Stall) {
        std::future::pending::<()>().await;
    }
    let hello = json!({ "op": HELLO_OP, "d": { "obsWebSocketVersion": "5.5.0", "rpcVersion": RPC_VERSION } });
    if !send_json(&mut socket, &hello).await {
        return;
    }
    loop {
        match next_json(&mut socket).await {
            Some(frame) if frame["op"].as_u64() == Some(IDENTIFY_OP) => break,
            Some(_) => {}
            None => return,
        }
    }
    let identified = json!({ "op": IDENTIFIED_OP, "d": { "negotiatedRpcVersion": RPC_VERSION } });
    if !send_json(&mut socket, &identified).await {
        return;
    }
    if matches!(handshake, Handshake::CloseAfterIdentified) {
        close_socket(&mut socket).await;
        return;
    }

    let mut seen_per_type: HashMap<String, usize> = HashMap::new();
    while let Some(frame) = next_json(&mut socket).await {
        if frame["op"].as_u64() != Some(REQUEST_OP) {
            continue;
        }
        let request_type = frame["d"]["requestType"].as_str().unwrap_or("").to_owned();
        let request_id = frame["d"]["requestId"].clone();
        log.0.lock().unwrap().requests.push((
            conn,
            request_type.clone(),
            frame["d"]["requestData"].clone(),
        ));
        let nth = seen_per_type.entry(request_type.clone()).or_default();
        let mut reply = script(Seen {
            conn,
            request_type: &request_type,
            nth: *nth,
        });
        *nth += 1;

        let response = |result: bool, code: u16, data: Value| {
            json!({ "op": RESPONSE_OP, "d": {
                "requestType": request_type,
                "requestId": request_id,
                "requestStatus": { "result": result, "code": code },
                "responseData": data,
            }})
        };
        loop {
            match reply {
                Reply::EventsFirst(events, then) => {
                    for event in &events {
                        if !send_json(&mut socket, event).await {
                            return;
                        }
                    }
                    reply = *then;
                }
                Reply::HeldUntil(gate, then) => {
                    gate.notified().await;
                    reply = *then;
                }
                Reply::Answer(data) => {
                    if !send_json(&mut socket, &response(true, OBS_SUCCESS, data)).await {
                        return;
                    }
                    break;
                }
                Reply::RejectWith(code) => {
                    if !send_json(&mut socket, &response(false, code, Value::Null)).await {
                        return;
                    }
                    break;
                }
                Reply::Reject => {
                    let rejection = response(false, OBS_RESOURCE_NOT_FOUND, Value::Null);
                    if !send_json(&mut socket, &rejection).await {
                        return;
                    }
                    break;
                }
                Reply::Silent => break,
                Reply::DropSocket => return,
                Reply::AnswerThenClose(data) => {
                    let _ = send_json(&mut socket, &response(true, OBS_SUCCESS, data)).await;
                    close_socket(&mut socket).await;
                    return;
                }
            }
        }
    }
}

pub(crate) fn completes_every_handshake(_: usize) -> Handshake {
    Handshake::Complete
}

pub(crate) async fn connect_to(fake: &FakeObs, recorder: &Recorder) -> Arc<ObsClient> {
    connect_with_timing(fake, recorder, fast_retries()).await
}

pub(crate) async fn connect_with_timing(
    fake: &FakeObs,
    recorder: &Recorder,
    timing: ObsTiming,
) -> Arc<ObsClient> {
    let client = ObsClient::connect_with_timing(
        &format!("127.0.0.1:{}", fake.port),
        None,
        Arc::new(recorder.clone()),
        timing,
    )
    .await
    .unwrap();
    Arc::new(client)
}

pub(crate) async fn connected(client: &ObsClient) {
    settle_until("the client reached connected", || {
        client.connection_state() == ConnectionState::Connected
    })
    .await;
}

pub(crate) async fn settle_until_async<F: Future<Output = bool>>(
    what: &str,
    mut done: impl FnMut() -> F,
) {
    let started = Instant::now();
    while !done().await {
        assert!(started.elapsed() < WALL_BUDGET, "never happened: {what}");
        tokio::time::sleep(REAL_CLOCK_POLL).await;
    }
}

pub(crate) async fn every_earlier_dial_was_accepted(port: u16) {
    let mut probe = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a server that already dropped the probe has accepted it"
    )]
    let _ = probe.write_all(NOT_A_WEB_SOCKET_UPGRADE).await;
    let mut ignored = Vec::new();
    let closed = tokio::time::timeout(WALL_BUDGET, probe.read_to_end(&mut ignored)).await;
    assert!(closed.is_ok(), "the server never reached the probe dial");
}

impl FakeObs {
    pub(crate) async fn dials_excluding_the_probe(&self) -> usize {
        every_earlier_dial_was_accepted(self.port).await;
        self.log.accepted() - 1
    }
}
