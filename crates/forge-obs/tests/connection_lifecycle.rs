#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    ChannelPublisher, FakeObs, Handshake, RELOOKED_ITEM_ID, Recorder, Reply, SCENE, SCENE_UUID,
    SNAPSHOT_ITEM_ID, SOURCE, Seen, WALL_BUDGET, bind_loopback, completes_every_handshake,
    connect_to, connect_with_timing, connected, event_frame, fast_retries, settle_until,
    settle_until_async, spawn_fake_obs, standard_reply, version_data,
};
use forge_events::{Event, EventPublisher};
use forge_events::{PlatformConnectionChanged, TypedEvent};
use forge_obs::{ObsClient, ObsError, ObsSink, ObsSource, ObsTiming};
use forge_platform_core::{BuiltinControl, BuiltinStatus, ConnectionState};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const DISCONNECT_HANG_GUARD: Duration = Duration::from_secs(20);

const BEYOND_ANY_GUARD: Duration = Duration::from_secs(100 * 24 * 3600);

const SHORT_REQUEST_TIMEOUT: Duration = Duration::from_millis(250);

const SHORT_CONNECT_TIMEOUT: Duration = Duration::from_millis(50);

const FAST_STATS_INTERVAL: Duration = Duration::from_millis(5);

const SHORT_STATS_TIMEOUT: Duration = Duration::from_millis(20);

const FIRST_ANSWERED_STATS_TIMEOUT: Duration = Duration::from_millis(100);

const UNANSWERED_POLLS_BEFORE_SUSPECT: usize = 3;

const HANDSHAKE_BUDGET: Duration = Duration::from_secs(30);

const ATTEMPTS_BEFORE_VERDICT: usize = 3;

const SESSION_INVALIDATED: u16 = 4011;

const EVENT_BURST: usize = 1100;

fn endless_retry_pause() -> ObsTiming {
    ObsTiming {
        reconnect_base_delay: BEYOND_ANY_GUARD,
        reconnect_max_delay: BEYOND_ANY_GUARD,
        ..ObsTiming::default()
    }
}

async fn serve_refused_handshakes(listener: TcpListener, accepted: mpsc::UnboundedSender<()>) {
    while let Ok((stream, _)) = listener.accept().await {
        drop(stream);
        if accepted.send(()).is_err() {
            return;
        }
    }
}

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
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = upgraded.send(());
    if gate.await.is_err() {
        return;
    }
    let frame = CloseFrame {
        code: CloseCode::from(SESSION_INVALIDATED),
        reason: "closed by the test peer".into(),
    };
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = socket.send(Message::Close(Some(frame))).await;
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = settled.send(());
    while let Some(Ok(_)) = socket.next().await {}
}

fn announced_states(rx: &mut mpsc::UnboundedReceiver<Event>) -> Vec<String> {
    let mut states = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if event.kind == PlatformConnectionChanged::KIND {
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

async fn next_accept(accepted: &mut mpsc::UnboundedReceiver<()>, attempt: usize) {
    let next = tokio::time::timeout(WALL_BUDGET, accepted.recv()).await;
    assert!(
        matches!(next, Ok(Some(()))),
        "the supervisor stopped retrying before attempt {attempt}"
    );
}

#[tokio::test]
async fn a_backoff_loop_announces_one_state_change_per_real_transition() {
    let (listener, port) = bind_loopback().await;
    let (accept_tx, mut accept_rx) = mpsc::unbounded_channel();
    let server = tokio::spawn(serve_refused_handshakes(listener, accept_tx));
    let (tx, mut rx) = mpsc::unbounded_channel();

    let client = ObsClient::connect_with_timing(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(ChannelPublisher(tx)),
        fast_retries(),
    )
    .await
    .unwrap();

    for attempt in 1..=ATTEMPTS_BEFORE_VERDICT {
        next_accept(&mut accept_rx, attempt).await;
    }
    client.disconnect().await.unwrap();
    server.abort();

    assert_eq!(
        announced_states(&mut rx),
        vec!["reconnecting".to_owned(), "disconnected".to_owned()],
        "{ATTEMPTS_BEFORE_VERDICT} refused attempts must announce one retry, then the shutdown"
    );
}

#[tokio::test]
async fn disconnecting_during_a_stalled_connect_does_not_wait_out_the_connect_timeout() {
    let (listener, port) = bind_loopback().await;
    let (dialed_tx, mut dialed) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            held.push(stream);
            if dialed_tx.send(()).is_err() {
                return;
            }
        }
    });
    let (tx, _rx) = mpsc::unbounded_channel();
    let client = ObsClient::connect_with_timing(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(ChannelPublisher(tx)),
        ObsTiming {
            connect_timeout: BEYOND_ANY_GUARD,
            ..ObsTiming::default()
        },
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(DISCONNECT_HANG_GUARD, dialed.recv())
            .await
            .is_ok_and(|dial| dial.is_some()),
        "the client never dialed"
    );

    let outcome = finishes_promptly(client.disconnect()).await;
    server.abort();

    assert!(
        outcome.is_some(),
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

    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = gate_tx.send(());
    tokio::time::timeout(HANDSHAKE_BUDGET, settled_rx)
        .await
        .expect("the mock peer never sent its close frame")
        .expect("the mock peer stopped before its close frame");
    drop(client);
    let published_after_retiring = tokio::time::timeout(HANDSHAKE_BUDGET, rx.recv())
        .await
        .expect("a task of the retired client kept its publisher alive");
    server.abort();

    assert!(
        published_after_retiring.is_none(),
        "a client whose disconnect already returned published again"
    );
}

async fn finishes_promptly<F: std::future::Future>(work: F) -> Option<F::Output> {
    tokio::time::timeout(DISCONNECT_HANG_GUARD, work).await.ok()
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

#[tokio::test]
async fn a_request_obs_never_answers_times_out_and_sends_the_session_back_to_reconnecting() {
    let mut request_timeout = SHORT_REQUEST_TIMEOUT;
    let (fake, recorder, client) = loop {
        let fake = spawn_fake_obs(
            completes_every_handshake,
            Arc::new(|seen: Seen<'_>| match seen.request_type {
                "SetCurrentProgramScene" => Reply::Silent,
                other => standard_reply(other),
            }),
        )
        .await;
        let recorder = Recorder::default();
        let timing = ObsTiming {
            request_timeout,
            ..fast_retries()
        };
        let client = connect_with_timing(&fake, &recorder, timing).await;
        connected(&client).await;
        if fake.log.accepted() == 1 {
            break (fake, recorder, client);
        }
        request_timeout *= 2;
        assert!(request_timeout < WALL_BUDGET, "no session ever settled");
    };

    let request = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.set_scene(SCENE).await }
    });
    settle_until("the switch reached OBS", || {
        fake.log.count(0, "SetCurrentProgramScene") == 1
    })
    .await;
    settle_until("the unanswered switch gave up", || request.is_finished()).await;

    let outcome = request.await.unwrap();
    assert!(matches!(outcome, Err(ObsError::Timeout)), "got {outcome:?}");
    settle_until("the supervisor dropped the silent session", || {
        recorder.states().len() >= 2
    })
    .await;
    assert_eq!(recorder.states()[..2], ["connected", "reconnecting"]);
}

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
    let timing = ObsTiming {
        request_timeout: BEYOND_ANY_GUARD,
        ..ObsTiming::default()
    };
    let client = connect_with_timing(&fake, &recorder, timing).await;
    settle_until("the catalog load asked OBS for its version", || {
        fake.log.count(0, "GetVersion") == 2
    })
    .await;

    let outcome = finishes_promptly(client.disconnect()).await;

    assert!(
        outcome.is_some(),
        "disconnect waited on the stalled catalog load"
    );
}

fn stats_polls(request_timeout: Duration) -> ObsTiming {
    ObsTiming {
        stats_poll_interval: FAST_STATS_INTERVAL,
        stats_poll_request_timeout: request_timeout,
        ..fast_retries()
    }
}

#[tokio::test]
async fn a_silent_stats_heartbeat_drops_the_session_after_the_third_missed_poll() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match seen.request_type {
            "GetStats" => Reply::Silent,
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_with_timing(&fake, &recorder, stats_polls(SHORT_STATS_TIMEOUT)).await;
    connected(&client).await;

    settle_until("the silent heartbeat dropped the session", || {
        recorder.states().len() >= 2
    })
    .await;
    settle_until("three polls reached the dropped session", || {
        fake.log.count(0, "GetStats") >= UNANSWERED_POLLS_BEFORE_SUSPECT
    })
    .await;

    assert_eq!(recorder.states()[..2], ["connected", "reconnecting"]);
}

#[tokio::test]
async fn two_missed_stats_polls_keep_the_session_connected() {
    const POLLS_AFTER_THE_MISSES: usize = 2;
    let mut request_timeout = FIRST_ANSWERED_STATS_TIMEOUT;
    loop {
        let fake = spawn_fake_obs(
            completes_every_handshake,
            Arc::new(
                |seen: Seen<'_>| match (seen.conn, seen.request_type, seen.nth) {
                    (0, "GetStats", 0 | 1) => Reply::Silent,
                    (_, other, _) => standard_reply(other),
                },
            ),
        )
        .await;
        let recorder = Recorder::default();
        let client = connect_with_timing(&fake, &recorder, stats_polls(request_timeout)).await;
        connected(&client).await;

        settle_until("the polls after the misses reached OBS", || {
            fake.log.count(0, "GetStats")
                > UNANSWERED_POLLS_BEFORE_SUSPECT - 1 + POLLS_AFTER_THE_MISSES
                || recorder.states().iter().any(|s| s == "reconnecting")
        })
        .await;

        if recorder.states() == ["connected"] {
            return;
        }
        request_timeout *= 2;
        assert!(
            request_timeout < WALL_BUDGET,
            "the session dropped before its third miss"
        );
    }
}

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
    settle_until("the supervisor dialled again after the drop", || {
        fake.log.accepted() >= 2
    })
    .await;
    settle_until("connection 1 reached its catalog load", || {
        fake.log.count(1, "GetSceneList") == 1
    })
    .await;
    settle_until("the supervisor dialled again after the failed load", || {
        fake.log.accepted() >= 3
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
    let (recorder, _client, _fake) = drive_a_drop_during_the_catalog_load().await;

    assert_eq!(recorder.states(), ["connected", "reconnecting"]);
}

#[tokio::test]
async fn a_connection_lost_during_the_catalog_load_keeps_the_last_good_catalog() {
    let (_recorder, client, _fake) = drive_a_drop_during_the_catalog_load().await;

    assert_eq!(client.scenes().await.unwrap(), [SCENE]);
    assert_eq!(client.sources(SCENE).await.unwrap().len(), 1);
}

#[tokio::test]
async fn obs_closing_right_after_the_handshake_is_retried_without_announcing_connected() {
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

        settle_until(label, || fake.log.accepted() >= 2).await;

        assert_eq!(recorder.states(), ["reconnecting"], "{label}");
    }
}

#[tokio::test]
async fn a_scene_item_toggled_during_the_catalog_load_lands_in_the_catalog_and_on_the_bus() {
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

fn scene_list(names: &[&str]) -> Reply {
    let scenes: Vec<Value> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            json!({ "sceneName": name, "sceneUuid": SCENE_UUID, "sceneIndex": index })
        })
        .collect();
    Reply::Answer(json!({ "scenes": scenes }))
}

async fn wait_for_scene(client: &ObsClient, scene: &str) {
    settle_until_async("the created scene appeared", || async {
        client.scenes().await.unwrap().iter().any(|s| s == scene)
    })
    .await;
}

#[tokio::test]
async fn a_scene_created_during_a_held_topology_fetch_survives_the_resync() {
    const INTRO: &str = "Intro";
    const LOADED: usize = 1;
    let release = Arc::new(Notify::new());
    let held = Arc::clone(&release);
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(move |seen: Seen<'_>| match seen.request_type {
            "GetSceneList" if seen.nth <= LOADED => scene_list(&[SCENE]),
            "GetSceneList" => scene_list(&[SCENE, INTRO]),
            "GetSceneItemList" if seen.nth == LOADED => Reply::EventsFirst(
                vec![event_frame(
                    "SceneCreated",
                    json!({ "sceneName": INTRO, "sceneUuid": SCENE_UUID, "isGroup": false }),
                )],
                Box::new(Reply::HeldUntil(
                    Arc::clone(&held),
                    Box::new(standard_reply("GetSceneItemList")),
                )),
            ),
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;

    client.request_catalog_resync();
    wait_for_scene(&client, INTRO).await;
    release.notify_one();

    settle_until_async("a refetch replaced the stale fetch", || async {
        !client.sources(INTRO).await.unwrap().is_empty()
    })
    .await;
    assert_eq!(client.scenes().await.unwrap(), vec![SCENE, INTRO]);
}

#[tokio::test]
async fn switching_the_scene_collection_makes_the_next_toggle_look_the_item_id_up_again() {
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
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "recv returns once the sender is dropped"
                    )]
                    let _ = gate.recv();
                });
            }
        }
        self.recorder.publish(event);
    }
}

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

    let probe = client.transitions().await;
    assert!(probe.is_err(), "precondition: the fake rejects the probe");
    drop(release);

    settle_until("the catalog was reloaded on the same session", || {
        fake.log.count(0, "GetSceneItemEnabled") >= 2
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

const PROBE_PASSWORD: &str = "obs-auto-reconnect-secret-4k2j";

struct RefusingObs {
    port: u16,
    attempts: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for RefusingObs {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl RefusingObs {
    async fn spawn() -> Self {
        let (listener, port) = bind_loopback().await;
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&attempts);
        let server = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        Self {
            port,
            attempts,
            server,
        }
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    async fn attempts_excluding_the_probe(&self) -> usize {
        common::every_earlier_dial_was_accepted(self.port).await;
        self.attempts() - 1
    }

    async fn connect(&self, password: Option<&str>) -> Arc<ObsClient> {
        self.connect_with_timing(password, fast_retries()).await
    }

    async fn connect_with_timing(
        &self,
        password: Option<&str>,
        timing: ObsTiming,
    ) -> Arc<ObsClient> {
        let client = ObsClient::connect_with_timing(
            &format!("127.0.0.1:{}", self.port),
            password,
            Arc::new(Recorder::default()),
            timing,
        )
        .await
        .unwrap();
        Arc::new(client)
    }
}

async fn disconnected(client: &ObsClient) {
    settle_until("the client settled disconnected", || {
        client.connection_state() == ConnectionState::Disconnected
    })
    .await;
}

async fn parked_after_one_refusal(fake: &RefusingObs, password: Option<&str>) -> Arc<ObsClient> {
    park(fake, fake.connect(password).await).await
}

async fn park(fake: &RefusingObs, client: Arc<ObsClient>) -> Arc<ObsClient> {
    client.set_auto_reconnect(false);
    disconnected(&client).await;
    assert_eq!(
        fake.attempts(),
        1,
        "precondition: one attempt before parking"
    );
    client
}

fn failure_code(client: &ObsClient) -> Option<String> {
    client
        .connect_failure()
        .map(|failure| failure.code().name().to_owned())
}

#[tokio::test]
async fn a_refused_first_connect_with_auto_reconnect_off_is_attempted_exactly_once() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;

    client.disconnect().await.unwrap();

    assert_eq!(fake.attempts_excluding_the_probe().await, 1);
}

#[tokio::test]
async fn turning_auto_reconnect_off_while_retrying_settles_disconnected_without_waiting_out_the_pause()
 {
    let fake = RefusingObs::spawn().await;
    let client = fake.connect_with_timing(None, endless_retry_pause()).await;
    settle_until("the first attempt failed", || {
        failure_code(&client).is_some()
    })
    .await;

    client.set_auto_reconnect(false);

    disconnected(&client).await;
}

#[tokio::test]
async fn turning_auto_reconnect_back_on_resumes_a_parked_supervisor_without_a_pause() {
    let fake = RefusingObs::spawn().await;
    let pausing = fake.connect_with_timing(None, endless_retry_pause()).await;
    let client = park(&fake, pausing).await;

    client.set_auto_reconnect(true);

    settle_until("the resumed supervisor dialled again", || {
        fake.attempts() == 2
    })
    .await;
}

#[tokio::test]
async fn a_manual_reconnect_with_auto_reconnect_off_makes_exactly_one_attempt() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;

    client.reconnect().await.unwrap();
    settle_until("the manual attempt failed and parked", || {
        fake.attempts() == 2 && client.connection_state() == ConnectionState::Disconnected
    })
    .await;
    client.disconnect().await.unwrap();

    assert_eq!(fake.attempts_excluding_the_probe().await, 2);
}

#[tokio::test]
async fn disconnecting_ends_a_parked_supervisor_so_turning_auto_reconnect_on_dials_nothing() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;

    finishes_promptly(client.disconnect())
        .await
        .expect("disconnect waited on the parked supervisor")
        .unwrap();
    client.set_auto_reconnect(true);

    assert_eq!(fake.attempts_excluding_the_probe().await, 1);
}

#[tokio::test]
async fn a_dropped_handshake_is_recorded_as_a_failure_that_never_carries_the_password() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, Some(PROBE_PASSWORD)).await;

    let failure = client.connect_failure().expect("no failure was recorded");

    assert!(
        !format!("{failure:?}").contains(PROBE_PASSWORD),
        "the recorded failure leaked the password"
    );
}

#[tokio::test]
async fn a_connect_to_a_closed_port_records_connect_refused() {
    let (listener, port) = bind_loopback().await;
    drop(listener);
    let client = ObsClient::connect_with_timing(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(Recorder::default()),
        fast_retries(),
    )
    .await
    .unwrap();
    client.set_auto_reconnect(false);
    disconnected(&client).await;

    assert_eq!(failure_code(&client).as_deref(), Some("connect_refused"));
}

#[tokio::test]
async fn disconnecting_clears_the_recorded_connect_failure() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;
    assert!(
        failure_code(&client).is_some(),
        "precondition: a failure is recorded"
    );

    client.disconnect().await.unwrap();

    assert_eq!(failure_code(&client), None);
}

#[tokio::test]
async fn a_manual_reconnect_clears_the_previous_failure_while_the_new_attempt_is_pending() {
    let (listener, port) = bind_loopback().await;
    let hold = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let accepted = Arc::new(AtomicUsize::new(0));
    let server = {
        let hold = Arc::clone(&hold);
        let accepted = Arc::clone(&accepted);
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                accepted.fetch_add(1, Ordering::SeqCst);
                if hold.load(Ordering::SeqCst) {
                    held.push(stream);
                }
            }
        })
    };
    let client = ObsClient::connect(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(Recorder::default()),
    )
    .await
    .unwrap();
    client.set_auto_reconnect(false);
    disconnected(&client).await;
    assert!(
        failure_code(&client).is_some(),
        "precondition: a failure is recorded"
    );
    hold.store(true, Ordering::SeqCst);

    client.reconnect().await.unwrap();
    settle_until("the second attempt reached the server", || {
        accepted.load(Ordering::SeqCst) == 2
    })
    .await;

    assert_eq!(failure_code(&client), None);
    server.abort();
}

#[tokio::test]
async fn a_successful_reconnect_clears_the_connection_lost_failure() {
    let fake = spawn_fake_obs(
        the_first_two_handshakes_complete,
        Arc::new(|seen: Seen<'_>| match (seen.conn, seen.request_type) {
            (0, "SetCurrentProgramScene") => Reply::DropSocket,
            (_, other) => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;
    let lost = client.set_scene(SCENE).await;
    assert!(matches!(lost, Err(ObsError::Disconnected)), "got {lost:?}");
    settle_until("the lost session was recorded", || {
        failure_code(&client).as_deref() == Some("connection_lost")
    })
    .await;

    settle_until("the second session connected", || {
        fake.log.accepted() == 2 && client.connection_state() == ConnectionState::Connected
    })
    .await;

    assert_eq!(failure_code(&client), None);
}
#[tokio::test]
async fn a_stalled_upgrade_records_unreachable_at_the_connect_timeout() {
    let fake = spawn_fake_obs(
        |_| Handshake::StallUpgrade,
        Arc::new(|seen: Seen<'_>| standard_reply(seen.request_type)),
    )
    .await;
    let recorder = Recorder::default();
    let timing = ObsTiming {
        connect_timeout: SHORT_CONNECT_TIMEOUT,
        ..fast_retries()
    };
    let client = connect_with_timing(&fake, &recorder, timing).await;
    client.set_auto_reconnect(false);

    settle_until("the stalled upgrade gave up", || {
        failure_code(&client).is_some()
    })
    .await;

    assert_eq!(failure_code(&client).as_deref(), Some("unreachable"));
}

#[tokio::test]
async fn a_live_session_lost_with_auto_reconnect_off_shows_why_and_is_not_redialled() {
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
    client.set_auto_reconnect(false);

    let lost = client.set_scene(SCENE).await;
    assert!(matches!(lost, Err(ObsError::Disconnected)), "got {lost:?}");
    disconnected(&client).await;
    let shown_failure = failure_code(&client);
    client.disconnect().await.unwrap();

    assert_eq!(
        fake.dials_excluding_the_probe().await,
        1,
        "the lost session was redialled"
    );
    assert_eq!(shown_failure.as_deref(), Some("connection_lost"));
}

#[derive(Debug, Clone, Copy)]
enum LeaveConnected {
    UserDisconnects,
    UserReconnects,
    ObsDropsTheSocket,
}

#[tokio::test]
async fn leaving_connected_is_flagged_expected_only_when_the_user_asked_for_it() {
    for (cause, want_state, want_expected) in [
        (LeaveConnected::UserDisconnects, "disconnected", true),
        (LeaveConnected::UserReconnects, "disconnected", true),
        (LeaveConnected::ObsDropsTheSocket, "reconnecting", false),
    ] {
        let fake = spawn_fake_obs(
            only_the_first_handshake_completes,
            Arc::new(|seen: Seen<'_>| match seen.request_type {
                "SetCurrentProgramScene" => Reply::DropSocket,
                other => standard_reply(other),
            }),
        )
        .await;
        let recorder = Recorder::default();
        let client = connect_to(&fake, &recorder).await;
        connected(&client).await;

        match cause {
            LeaveConnected::UserDisconnects => client.disconnect().await.unwrap(),
            LeaveConnected::UserReconnects => client.reconnect().await.unwrap(),
            LeaveConnected::ObsDropsTheSocket => {
                let lost = client.set_scene(SCENE).await;
                assert!(matches!(lost, Err(ObsError::Disconnected)), "got {lost:?}");
            }
        }
        settle_until("the client announced leaving connected", || {
            recorder.announcements().len() >= 2
        })
        .await;

        assert_eq!(
            recorder.announcements()[..2],
            [
                ("connected".to_owned(), Some(false)),
                (want_state.to_owned(), Some(want_expected)),
            ],
            "{cause:?}"
        );
    }
}

#[tokio::test]
async fn a_reconnect_to_an_upgraded_obs_reports_the_new_version() {
    const UPGRADED: &str = "32.0.0";
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match (seen.conn, seen.request_type) {
            (0, "GetSceneList") => Reply::DropSocket,
            (1.., "GetVersion") => {
                let mut upgraded = version_data();
                upgraded["obsVersion"] = json!(UPGRADED);
                Reply::Answer(upgraded)
            }
            (_, other) => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;

    settle_until("the second session reported its version", || {
        client.version().as_deref() == Some(UPGRADED)
    })
    .await;

    assert!(fake.log.accepted() >= 2);
}
