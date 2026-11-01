mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    HANG_GUARD, Handshake, Recorder, Reply, SCENE, Seen, bind_loopback, completes_every_handshake,
    connect_to, connect_with_timing, connected, fast_retries, finishes_promptly,
    only_the_first_handshake_completes, spawn_fake_obs, standard_reply, version_data,
};
use forge_events::test_support::ForwardingPublisher;
use forge_events::{Event, PlatformConnectionChanged, TypedEvent};
use forge_obs::{ObsClient, ObsError, ObsSink, ObsTiming};
use forge_platform_core::{BuiltinControl, BuiltinStatus};
use forge_test_support::settle_until;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const BEYOND_ANY_GUARD: Duration = Duration::from_secs(100 * 24 * 3600);
const SHORT_REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
const FAST_STATS_INTERVAL: Duration = Duration::from_millis(5);
const SHORT_STATS_TIMEOUT: Duration = Duration::from_millis(20);
const FIRST_ANSWERED_STATS_TIMEOUT: Duration = Duration::from_millis(100);
const UNANSWERED_POLLS_BEFORE_SUSPECT: usize = 3;
const ATTEMPTS_BEFORE_VERDICT: usize = 3;
const SESSION_INVALIDATED: u16 = 4011;

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
    let next = tokio::time::timeout(HANG_GUARD, accepted.recv()).await;
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
        Arc::new(ForwardingPublisher(tx)),
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
        Arc::new(ForwardingPublisher(tx)),
        ObsTiming {
            connect_timeout: BEYOND_ANY_GUARD,
            ..ObsTiming::default()
        },
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(HANG_GUARD, dialed.recv())
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
        Arc::new(ForwardingPublisher(tx)),
    )
    .await
    .unwrap();
    tokio::time::timeout(HANG_GUARD, upgraded_rx)
        .await
        .expect("the mock peer never reached the web-socket upgrade")
        .expect("the mock peer never completed the web-socket upgrade");
    tokio::time::timeout(HANG_GUARD, client.disconnect())
        .await
        .expect("retiring the client never returned")
        .unwrap();
    while rx.try_recv().is_ok() {}

    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = gate_tx.send(());
    tokio::time::timeout(HANG_GUARD, settled_rx)
        .await
        .expect("the mock peer never sent its close frame")
        .expect("the mock peer stopped before its close frame");
    drop(client);
    let published_after_retiring = tokio::time::timeout(HANG_GUARD, rx.recv())
        .await
        .expect("a task of the retired client kept its publisher alive");
    server.abort();

    assert!(
        published_after_retiring.is_none(),
        "a client whose disconnect already returned published again"
    );
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
        assert!(request_timeout < HANG_GUARD, "no session ever settled");
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
            request_timeout < HANG_GUARD,
            "the session dropped before its third miss"
        );
    }
}

fn only_the_first_handshake_closes_after_identified(conn: usize) -> Handshake {
    if conn == 0 {
        Handshake::CloseAfterIdentified
    } else {
        Handshake::Stall
    }
}

#[tokio::test]
async fn obs_closing_right_after_the_handshake_is_retried_without_announcing_connected() {
    for (label, handshake) in [
        (
            "close after Identified",
            only_the_first_handshake_closes_after_identified as fn(usize) -> Handshake,
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
        matches!(outcome, Err(ObsError::NotConnected)),
        "got {outcome:?}"
    );
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
                assert!(matches!(lost, Err(ObsError::NotConnected)), "got {lost:?}");
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
