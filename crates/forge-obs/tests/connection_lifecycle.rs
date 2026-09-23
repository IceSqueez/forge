#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_obs::ObsClient;
use forge_platform_core::{BuiltinControl, CONNECTION_STATE_CHANGED_KIND};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
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
