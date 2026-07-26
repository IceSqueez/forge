#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_obs::{ObsClient, ObsError};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const AUTHENTICATION_FAILED: u16 = 4009;
const SESSION_INVALIDATED: u16 = 4011;
const POLICY_VIOLATION: u16 = 1008;

const PROBE_PASSWORD: &str = "obs-handshake-secret-9z8y7x";

/// A loopback web-socket peer that completes the upgrade and then closes the connection with the
/// given code, which is how obs-websocket rejects an `Identify` it does not accept.
async fn serve_one_close_frame(listener: TcpListener, code: u16, reason: &'static str) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let frame = CloseFrame {
        code: CloseCode::from(code),
        reason: reason.into(),
    };
    if socket.send(Message::Close(Some(frame))).await.is_err() {
        return;
    }
    while let Some(Ok(_)) = socket.next().await {}
}

async fn bind_loopback() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

async fn probe_against_closing_server(code: u16, reason: &'static str, password: &str) -> ObsError {
    let (listener, port) = bind_loopback().await;
    let server = tokio::spawn(serve_one_close_frame(listener, code, reason));
    let outcome = forge_obs::probe_connection("127.0.0.1", port, password).await;
    server.abort();
    match outcome {
        Err(e) => e,
        Ok(_) => panic!("the probe completed a handshake against a peer that only closes"),
    }
}

struct ChannelPublisher(mpsc::UnboundedSender<Event>);

impl EventPublisher for ChannelPublisher {
    fn publish(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

#[tokio::test]
async fn only_the_obs_authentication_close_code_is_reported_as_an_auth_rejection() {
    for (code, expected_auth) in [
        (AUTHENTICATION_FAILED, true),
        (SESSION_INVALIDATED, false),
        (POLICY_VIOLATION, false),
    ] {
        let error = probe_against_closing_server(code, "closed by the test peer", "").await;
        assert_eq!(
            matches!(error, ObsError::Authentication),
            expected_auth,
            "close code {code} produced {error:?}"
        );
    }
}

#[tokio::test]
async fn a_connect_failure_reports_the_close_reason_the_server_sent() {
    let error =
        probe_against_closing_server(SESSION_INVALIDATED, "session invalidated by kick", "").await;
    let ObsError::Connect(detail) = &error else {
        panic!("expected a connect failure, got {error:?}");
    };
    assert!(
        detail.contains("session invalidated by kick"),
        "the underlying cause was dropped from: {detail}"
    );
}

// Why: this is the second path (after a refused connect) that renders an OBS failure verbatim in
// the setup banner, and it now stringifies the whole error chain rather than one fixed sentence.
#[tokio::test]
async fn a_rejected_handshake_keeps_the_password_out_of_the_error_text() {
    let error = probe_against_closing_server(
        SESSION_INVALIDATED,
        "closed by the test peer",
        PROBE_PASSWORD,
    )
    .await;
    assert!(
        !format!("{error}").contains(PROBE_PASSWORD),
        "the handshake error leaked the password: {error}"
    );
    assert!(
        !format!("{error:?}").contains(PROBE_PASSWORD),
        "the handshake error Debug leaked the password"
    );
}

// Why: a wrong password used to arrive as a generic connect failure, which left the supervisor's
// authentication guard unreachable and retried the rejected password on the backoff loop forever.
#[tokio::test]
async fn a_rejected_password_makes_the_supervisor_report_an_authentication_failure() {
    let (listener, port) = bind_loopback().await;
    let server = tokio::spawn(serve_one_close_frame(
        listener,
        AUTHENTICATION_FAILED,
        "authentication failed",
    ));
    let (tx, mut rx) = mpsc::unbounded_channel();

    let client = ObsClient::connect(
        &format!("127.0.0.1:{port}"),
        Some("wrong-password"),
        Arc::new(ChannelPublisher(tx)),
    )
    .await
    .unwrap();

    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("the supervisor published nothing before the timeout")
        .expect("the publisher channel closed without an event");

    server.abort();
    drop(client);

    assert_eq!(event.kind, "obs.connection.auth_failed");
}
