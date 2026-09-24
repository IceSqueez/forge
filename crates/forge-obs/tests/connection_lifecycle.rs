#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::{Event, EventPublisher};
use forge_obs::{ObsClient, ObsError, ObsSink, ObsSource};
use forge_platform_core::{BuiltinControl, CONNECTION_STATE_CHANGED_KIND, ConnectionState};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

/// RFC 5737 TEST-NET-1: a handshake sent here is never answered, so the connect stays in flight.
const UNROUTABLE_HOST: &str = "192.0.2.1";

const OBS_WEBSOCKET_PORT: u16 = 4455;

/// Far below `obws`' own connect timeout, so waiting one out fails this instead of passing slowly.
const PROMPT_DISCONNECT: Duration = Duration::from_secs(2);

const RETRY_BUDGET: Duration = Duration::from_secs(600);

/// How long the retired peer is given to push its close frame; the assertion holds either way.
const PEER_SPEAKS_WINDOW: Duration = Duration::from_secs(2);

const HANDSHAKE_BUDGET: Duration = Duration::from_secs(30);

const ATTEMPTS_BEFORE_VERDICT: usize = 3;

const SESSION_INVALIDATED: u16 = 4011;

struct ChannelPublisher(mpsc::UnboundedSender<Event>);

impl EventPublisher for ChannelPublisher {
    fn publish(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

async fn bind_loopback() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// Accepts the TCP connection and drops it before any web-socket upgrade, which is how a port that
/// is listening but is not obs-websocket refuses the supervisor on every retry.
async fn serve_refused_handshakes(listener: TcpListener, accepted: mpsc::UnboundedSender<()>) {
    while let Ok((stream, _)) = listener.accept().await {
        drop(stream);
        if accepted.send(()).is_err() {
            return;
        }
    }
}

/// Completes the upgrade, announces it on `upgraded`, then stays silent until `gate` fires, so the
/// test can retire an established session before the peer says anything at all.
async fn serve_gated_close_frame(
    listener: TcpListener,
    upgraded: oneshot::Sender<()>,
    gate: oneshot::Receiver<()>,
    settled: oneshot::Sender<()>,
) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let _ = upgraded.send(());
    if gate.await.is_err() {
        return;
    }
    let frame = CloseFrame {
        code: CloseCode::from(SESSION_INVALIDATED),
        reason: "closed by the test peer".into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
    let _ = settled.send(());
    while let Some(Ok(_)) = socket.next().await {}
}

fn announced_states(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<String> {
    let mut states = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if event.kind == CONNECTION_STATE_CHANGED_KIND {
            states.push(
                event.payload["state"]
                    .as_str()
                    .unwrap_or("<missing>")
                    .to_owned(),
            );
        }
    }
    states
}

// Why: the integration header reloads off `platform.connection.changed`, so a retry loop that
// re-announced `reconnecting` on every attempt would redraw the whole screen once per backoff tick.
#[tokio::test]
async fn a_backoff_loop_announces_one_state_change_per_real_transition() {
    tokio::time::pause();
    let (listener, port) = bind_loopback().await;
    let (accept_tx, mut accept_rx) = mpsc::unbounded_channel();
    let server = tokio::spawn(serve_refused_handshakes(listener, accept_tx));
    let (tx, mut rx) = mpsc::unbounded_channel();

    let client = ObsClient::connect(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(ChannelPublisher(tx)),
    )
    .await
    .unwrap();

    for attempt in 1..=ATTEMPTS_BEFORE_VERDICT {
        tokio::time::timeout(RETRY_BUDGET, accept_rx.recv())
            .await
            .unwrap_or_else(|_| panic!("the supervisor stopped retrying before attempt {attempt}"))
            .unwrap_or_else(|| panic!("the mock server stopped before attempt {attempt}"));
    }
    client.disconnect().await.unwrap();
    server.abort();

    assert_eq!(
        announced_states(&mut rx),
        vec!["reconnecting".to_owned(), "disconnected".to_owned()],
        "{ATTEMPTS_BEFORE_VERDICT} refused attempts must announce one retry, then the shutdown"
    );
}

// Why: "Save & Reconnect" retires the live client before the new one connects, so a disconnect
// raised while the old client is still dialling has to cancel the dial rather than queue behind it.
#[tokio::test]
async fn disconnecting_during_an_unreachable_connect_does_not_wait_out_the_connect_timeout() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let client = ObsClient::connect(
        &format!("{UNROUTABLE_HOST}:{OBS_WEBSOCKET_PORT}"),
        None,
        Arc::new(ChannelPublisher(tx)),
    )
    .await
    .unwrap();
    tokio::task::yield_now().await;

    let outcome = tokio::time::timeout(PROMPT_DISCONNECT, client.disconnect()).await;

    assert!(
        outcome.is_ok(),
        "disconnect queued behind the connect instead of cancelling it"
    );
}

#[tokio::test]
async fn a_retired_client_stays_silent_when_its_server_speaks_afterwards() {
    let (listener, port) = bind_loopback().await;
    let (upgraded_tx, upgraded_rx) = oneshot::channel();
    let (gate_tx, gate_rx) = oneshot::channel();
    let (settled_tx, settled_rx) = oneshot::channel();
    let server = tokio::spawn(serve_gated_close_frame(
        listener,
        upgraded_tx,
        gate_rx,
        settled_tx,
    ));
    let (tx, mut rx) = mpsc::unbounded_channel();

    let client = ObsClient::connect(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(ChannelPublisher(tx)),
    )
    .await
    .unwrap();
    tokio::time::timeout(HANDSHAKE_BUDGET, upgraded_rx)
        .await
        .expect("the mock peer never reached the web-socket upgrade")
        .expect("the mock peer never completed the web-socket upgrade");
    tokio::time::timeout(HANDSHAKE_BUDGET, client.disconnect())
        .await
        .expect("retiring the client never returned")
        .unwrap();
    while rx.try_recv().is_ok() {}

    let _ = gate_tx.send(());
    let _ = tokio::time::timeout(PEER_SPEAKS_WINDOW, settled_rx).await;
    tokio::task::yield_now().await;
    server.abort();

    assert!(
        rx.try_recv().is_err(),
        "a client whose disconnect already returned published again"
    );
}

/// Wall-clock ceiling for every wait below. The waits themselves never sleep; this only turns a
/// regression that would hang forever into a failure.
const WALL_BUDGET: Duration = Duration::from_secs(20);

/// Virtual time added per step while a test drives the paused clock towards a deadline; far below
/// every request deadline, so an answered request never races its own timeout.
const VIRTUAL_STEP: Duration = Duration::from_millis(50);

const REAL_CLOCK_POLL: Duration = Duration::from_millis(10);

const SCENE: &str = "Main";
const SCENE_UUID: &str = "6f1d3c1e-8a3b-4b8e-9d1a-0c2b3a4d5e6f";
const SOURCE: &str = "Webcam";
const SNAPSHOT_ITEM_ID: i64 = 7;
const RELOOKED_ITEM_ID: i64 = 9;

/// More than the supervisor's event buffer holds, so a burst this size overflows it.
const EVENT_BURST: usize = 1100;

const REQUEST_OP: u64 = 6;
const IDENTIFY_OP: u64 = 1;
const OBS_SUCCESS: u16 = 100;
const OBS_RESOURCE_NOT_FOUND: u16 = 600;

/// Keeps the paused clock from auto-advancing: tokio does not auto-advance a current-thread
/// runtime while a blocking task is alive, so virtual time only moves on `tokio::time::advance`.
/// Why: with auto-advance on, every loopback round trip parks the runtime, and each park would
/// jump the clock to the next request deadline before the answer is read.
struct FrozenClock(#[allow(dead_code)] std::sync::mpsc::Sender<()>);

fn freeze_clock() -> FrozenClock {
    tokio::time::pause();
    let (release, held) = std::sync::mpsc::channel::<()>();
    tokio::task::spawn_blocking(move || {
        let _ = held.recv();
    });
    FrozenClock(release)
}

async fn settle_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < WALL_BUDGET, "never happened: {what}");
        tokio::task::yield_now().await;
    }
}

async fn advance_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < WALL_BUDGET, "never happened: {what}");
        tokio::time::advance(VIRTUAL_STEP).await;
        tokio::task::yield_now().await;
    }
}

#[derive(Clone, Copy)]
enum Handshake {
    Complete,
    CloseAfterIdentified,
    /// Upgrades the socket and never says `Hello`, parking the next attempt mid-dial.
    Stall,
}

enum Reply {
    Answer(Value),
    Reject,
    Silent,
    /// Ends the TCP connection without a close frame, the way a crashed OBS does.
    DropSocket,
    AnswerThenClose(Value),
    EventsFirst(Vec<Value>, Box<Reply>),
}

#[derive(Clone, Copy)]
struct Seen<'a> {
    conn: usize,
    request_type: &'a str,
    nth: usize,
}

type Script = Arc<dyn Fn(Seen<'_>) -> Reply + Send + Sync>;

#[derive(Clone, Default)]
struct WireLog(Arc<Mutex<WireState>>);

#[derive(Default)]
struct WireState {
    accepted: usize,
    requests: Vec<(usize, String, Value)>,
}

impl WireLog {
    fn accepted(&self) -> usize {
        self.0.lock().unwrap().accepted
    }

    fn count(&self, conn: usize, request_type: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(c, t, _)| *c == conn && t == request_type)
            .count()
    }

    fn requests_of(&self, conn: usize, types: &[&str]) -> Vec<(String, Value)> {
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
struct Recorder(Arc<Mutex<Vec<Event>>>);

impl EventPublisher for Recorder {
    fn publish(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Recorder {
    fn kinds(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind.clone())
            .collect()
    }

    fn states(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == CONNECTION_STATE_CHANGED_KIND)
            .map(|e| {
                e.payload["state"]
                    .as_str()
                    .unwrap_or("<missing>")
                    .to_owned()
            })
            .collect()
    }
}

fn version_data() -> Value {
    json!({
        "obsVersion": "31.0.0",
        "obsWebSocketVersion": "5.5.0",
        "rpcVersion": 1,
        "availableRequests": [],
        "supportedImageFormats": [],
        "platform": "linux",
        "platformDescription": "loopback fake",
    })
}

/// One scene holding one non-audio source; everything the catalog load does not need is
/// rejected, which the load tolerates.
fn standard_reply(request_type: &str) -> Reply {
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

fn event_frame(event_type: &str, data: Value) -> Value {
    json!({ "op": 5, "d": { "eventType": event_type, "eventData": data } })
}

struct FakeObs {
    port: u16,
    log: WireLog,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for FakeObs {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn spawn_fake_obs(handshake: fn(usize) -> Handshake, script: Script) -> FakeObs {
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
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    if matches!(handshake, Handshake::Stall) {
        std::future::pending::<()>().await;
    }
    let hello = json!({ "op": 0, "d": { "obsWebSocketVersion": "5.5.0", "rpcVersion": 1 } });
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
    let identified = json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } });
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
            json!({ "op": 7, "d": {
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
                Reply::Answer(data) => {
                    if !send_json(&mut socket, &response(true, OBS_SUCCESS, data)).await {
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

fn completes_every_handshake(_: usize) -> Handshake {
    Handshake::Complete
}

fn the_first_two_handshakes_complete(conn: usize) -> Handshake {
    if conn < 2 {
        Handshake::Complete
    } else {
        Handshake::Stall
    }
}

fn only_the_first_handshake_completes(conn: usize) -> Handshake {
    if conn == 0 {
        Handshake::Complete
    } else {
        Handshake::Stall
    }
}

async fn connect_to(fake: &FakeObs, recorder: &Recorder) -> Arc<ObsClient> {
    let client = ObsClient::connect(
        &format!("127.0.0.1:{}", fake.port),
        None,
        Arc::new(recorder.clone()),
    )
    .await
    .unwrap();
    Arc::new(client)
}

async fn connected(client: &ObsClient) {
    settle_until("the client reached connected", || {
        client.connection_state() == ConnectionState::Connected
    })
    .await;
}

fn scene_item_enable_state_changed(enabled: bool) -> Value {
    event_frame(
        "SceneItemEnableStateChanged",
        json!({
            "sceneName": SCENE,
            "sceneUuid": SCENE_UUID,
            "sceneItemId": SNAPSHOT_ITEM_ID,
            "sceneItemEnabled": enabled,
        }),
    )
}

// Why: a chat-triggered step on a serial queue awaits this request; without a deadline a crashed
// or stalled OBS freezes that queue until forge restarts.
#[tokio::test]
async fn a_request_obs_never_answers_times_out_and_sends_the_session_back_to_reconnecting() {
    let _clock = freeze_clock();
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match seen.request_type {
            "SetCurrentProgramScene" => Reply::Silent,
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    let request = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.set_scene(SCENE).await }
    });
    settle_until("the switch reached OBS", || {
        fake.log.count(0, "SetCurrentProgramScene") == 1
    })
    .await;
    advance_until("the unanswered switch gave up", || request.is_finished()).await;

    let outcome = request.await.unwrap();
    assert!(matches!(outcome, Err(ObsError::Timeout)), "got {outcome:?}");
    settle_until("the supervisor dropped the silent session", || {
        recorder.states() == ["connected", "reconnecting"]
    })
    .await;
}

// Why: "Reconnect" / "Disconnect" is exactly what the user clicks when OBS hangs; it must not
// queue behind a catalog load that is waiting on OBS.
#[tokio::test]
async fn disconnecting_while_the_catalog_load_waits_on_obs_returns_promptly() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match (seen.request_type, seen.nth) {
            ("GetVersion", 0) => Reply::Answer(version_data()),
            _ => Reply::Silent,
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    settle_until("the catalog load asked OBS for its version", || {
        fake.log.count(0, "GetVersion") == 2
    })
    .await;

    let outcome = tokio::time::timeout(PROMPT_DISCONNECT, client.disconnect()).await;

    assert!(
        outcome.is_ok(),
        "disconnect waited on the stalled catalog load"
    );
}

/// Real time: under the frozen clock the stats poll's concurrent requests stall on the loopback
/// socket (a harness artefact; the same poll runs normally on the real clock), so this one waits
/// out the production poll cadence instead.
async fn wait_on_the_real_clock(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < WALL_BUDGET, "never happened: {what}");
        tokio::time::sleep(REAL_CLOCK_POLL).await;
    }
}

// Why: the stats poll is the only heartbeat on a half-open LAN link (OBS PC loses power), where
// no request ever fails on its own before the OS gives up on TCP.
#[tokio::test]
async fn a_silent_stats_heartbeat_sends_the_session_back_to_reconnecting() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match seen.request_type {
            "GetStats" => Reply::Silent,
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    wait_on_the_real_clock("the silent heartbeat dropped the session", || {
        recorder.states().len() >= 2
    })
    .await;

    assert_eq!(recorder.states()[..2], ["connected", "reconnecting"]);
}

// Why: one slow poll while OBS loads a scene must not tear a healthy session down mid-stream.
#[tokio::test]
async fn a_single_missed_stats_poll_keeps_the_session_connected() {
    const POLLS_AFTER_THE_MISS: usize = 2;
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(
            |seen: Seen<'_>| match (seen.conn, seen.request_type, seen.nth) {
                (0, "GetStats", 0) => Reply::Silent,
                (_, other, _) => standard_reply(other),
            },
        ),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    wait_on_the_real_clock("the poll after the miss reached OBS", || {
        fake.log.count(0, "GetStats") > POLLS_AFTER_THE_MISS
            || recorder.states().iter().any(|s| s == "reconnecting")
    })
    .await;

    assert_eq!(recorder.states(), ["connected"]);
}

/// Connection 0 loads a catalog, then drops its socket on a scene switch; connection 1 drops its
/// socket in the middle of the catalog load; the dial for connection 2 is where the test stops.
async fn drive_a_drop_during_the_catalog_load() -> (Recorder, Arc<ObsClient>, FakeObs) {
    let fake = spawn_fake_obs(
        the_first_two_handshakes_complete,
        Arc::new(|seen: Seen<'_>| match (seen.conn, seen.request_type) {
            (0, "SetCurrentProgramScene") | (1, "GetSceneList") => Reply::DropSocket,
            (_, other) => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;
    assert_eq!(client.scenes().await.unwrap(), [SCENE], "precondition");

    let lost = client.set_scene(SCENE).await;
    assert!(matches!(lost, Err(ObsError::Disconnected)), "got {lost:?}");
    advance_until("the supervisor dialled again after the failed load", || {
        fake.log.accepted() == 3
    })
    .await;
    assert_eq!(
        fake.log.count(1, "GetSceneList"),
        1,
        "precondition: connection 1 died during its catalog load"
    );
    (recorder, client, fake)
}

#[tokio::test]
async fn a_connection_lost_during_the_catalog_load_is_retried_without_announcing_connected() {
    let _clock = freeze_clock();
    let (recorder, _client, _fake) = drive_a_drop_during_the_catalog_load().await;

    assert_eq!(recorder.states(), ["connected", "reconnecting"]);
}

#[tokio::test]
async fn a_connection_lost_during_the_catalog_load_keeps_the_last_good_catalog() {
    let _clock = freeze_clock();
    let (_recorder, client, _fake) = drive_a_drop_during_the_catalog_load().await;

    assert_eq!(client.scenes().await.unwrap(), [SCENE]);
    assert_eq!(client.sources(SCENE).await.unwrap().len(), 1);
}

#[tokio::test]
async fn obs_closing_right_after_the_handshake_is_retried_without_announcing_connected() {
    let _clock = freeze_clock();
    for (label, handshake) in [
        (
            "close after Identified",
            (|conn| {
                if conn == 0 {
                    Handshake::CloseAfterIdentified
                } else {
                    Handshake::Stall
                }
            }) as fn(usize) -> Handshake,
        ),
        (
            "close after the version check",
            only_the_first_handshake_completes,
        ),
    ] {
        let fake = spawn_fake_obs(
            handshake,
            Arc::new(|seen: Seen<'_>| match seen.request_type {
                "GetVersion" => Reply::AnswerThenClose(version_data()),
                other => standard_reply(other),
            }),
        )
        .await;
        let recorder = Recorder::default();
        let _client = connect_to(&fake, &recorder).await;

        advance_until(label, || fake.log.accepted() == 2).await;

        assert_eq!(recorder.states(), ["reconnecting"], "{label}");
    }
}

// Why: the post-connect catalog load is hundreds of round trips on a large collection; a source
// the streamer hides in OBS meanwhile must not stay "visible" in forge until the next reconnect.
#[tokio::test]
async fn a_scene_item_toggled_during_the_catalog_load_lands_in_the_catalog_and_on_the_bus() {
    let _clock = freeze_clock();
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(
            |seen: Seen<'_>| match (seen.conn, seen.request_type, seen.nth) {
                (0, "GetSceneItemEnabled", 0) => Reply::EventsFirst(
                    vec![scene_item_enable_state_changed(false)],
                    Box::new(Reply::Answer(json!({ "sceneItemEnabled": true }))),
                ),
                (_, other, _) => standard_reply(other),
            },
        ),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    settle_until("the toggle reached the bus", || {
        recorder
            .kinds()
            .iter()
            .any(|k| k == "obs.source.visibility_changed")
    })
    .await;

    let rows = client.sources(SCENE).await.unwrap();
    assert!(
        rows.iter().any(|row| row.name == SOURCE && !row.visible),
        "the catalog kept the snapshot's stale visibility: {rows:?}"
    );
}

#[tokio::test]
async fn switching_the_scene_collection_makes_the_next_toggle_look_the_item_id_up_again() {
    let _clock = freeze_clock();
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| standard_reply(seen.request_type)),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    client
        .set_source_visible(SCENE, SOURCE, false)
        .await
        .unwrap();
    client.set_current_scene_collection("Collab").await.unwrap();
    client
        .set_source_visible(SCENE, SOURCE, true)
        .await
        .unwrap();

    let toggles: Vec<(String, Option<i64>)> = fake
        .log
        .requests_of(0, &["GetSceneItemId", "SetSceneItemEnabled"])
        .into_iter()
        .map(|(t, data)| (t, data["sceneItemId"].as_i64()))
        .collect();
    assert_eq!(
        toggles,
        [
            ("SetSceneItemEnabled".to_owned(), Some(SNAPSHOT_ITEM_ID)),
            ("GetSceneItemId".to_owned(), None),
            ("SetSceneItemEnabled".to_owned(), Some(RELOOKED_ITEM_ID)),
        ]
    );
}

#[tokio::test]
async fn a_request_in_flight_when_obs_drops_the_socket_fails_as_disconnected() {
    let _clock = freeze_clock();
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match seen.request_type {
            "SetCurrentProgramScene" => Reply::DropSocket,
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    let outcome = client.set_scene(SCENE).await;

    assert!(
        matches!(outcome, Err(ObsError::Disconnected)),
        "got {outcome:?}"
    );
}

/// Blocks the supervisor inside its first scene-change publish until released, so OBS events pile
/// up in the client's buffer the way they do when the runtime is starved.
struct StallingPublisher {
    recorder: Recorder,
    gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl EventPublisher for StallingPublisher {
    fn publish(&self, event: Event) {
        if event.kind == "obs.scene.changed" {
            let gate = self.gate.lock().unwrap().take();
            if let Some(gate) = gate {
                tokio::task::block_in_place(|| {
                    let _ = gate.recv();
                });
            }
        }
        self.recorder.publish(event);
    }
}

// Why: obws reports an overflowed event buffer the same way as a closed socket; tearing a healthy
// session down for it would drop every event of the reconnect gap mid-stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_burst_that_overflows_the_buffer_resyncs_the_catalog_on_the_same_session() {
    let scene_changed = event_frame(
        "CurrentProgramSceneChanged",
        json!({ "sceneName": SCENE, "sceneUuid": SCENE_UUID }),
    );
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(move |seen: Seen<'_>| match seen.request_type {
            "GetSceneTransitionList" => Reply::EventsFirst(
                vec![scene_changed.clone(); EVENT_BURST],
                Box::new(Reply::Reject),
            ),
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let (release, gate) = std::sync::mpsc::channel();
    let publisher = Arc::new(StallingPublisher {
        recorder: recorder.clone(),
        gate: Mutex::new(Some(gate)),
    });
    let client = ObsClient::connect(&format!("127.0.0.1:{}", fake.port), None, publisher)
        .await
        .unwrap();
    connected(&client).await;
    assert_eq!(fake.log.count(0, "GetSceneItemEnabled"), 1, "precondition");

    // The response trails the burst on the wire, so once it is back every burst event has been
    // buffered while the supervisor was stalled.
    let probe = client.transitions().await;
    assert!(probe.is_err(), "precondition: the fake rejects the probe");
    drop(release);

    settle_until("the catalog was reloaded on the same session", || {
        fake.log.count(0, "GetSceneItemEnabled") == 2
    })
    .await;
    assert!(
        !recorder
            .kinds()
            .iter()
            .any(|k| k == "obs.connection.disconnected"),
        "the overflow was announced as a lost connection"
    );
    assert_eq!(fake.log.accepted(), 1, "the overflow redialled OBS");
}
