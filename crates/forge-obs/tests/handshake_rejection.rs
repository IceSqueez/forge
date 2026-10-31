#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use common::{ChannelPublisher, bind_loopback, settle_until};
use forge_events::{Event, EventPublisher};
use forge_obs::{ObsClient, ObsError};
use forge_platform_core::ConnectionState;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

const AUTHENTICATION_FAILED: u16 = 4009;
const SESSION_INVALIDATED: u16 = 4011;
const POLICY_VIOLATION: u16 = 1008;

const PROBE_PASSWORD: &str = "obs-handshake-secret-9z8y7x";

const AUTH_FAILED_KIND: &str = "obs.connection.auth_failed";

const SUPERVISOR_BUDGET: Duration = Duration::from_secs(30);

async fn serve_one_close_frame(listener: TcpListener, code: u16, reason: &'static str) {
    serve_one_gated_close_frame(listener, code, reason, None).await;
}

async fn serve_one_gated_close_frame(
    listener: TcpListener,
    code: u16,
    reason: &'static str,
    gate: Option<oneshot::Receiver<()>>,
) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    if let Some(gate) = gate
        && gate.await.is_err()
    {
        return;
    }
    let frame = CloseFrame {
        code: CloseCode::from(code),
        reason: reason.into(),
    };
    if socket.send(Message::Close(Some(frame))).await.is_err() {
        return;
    }
    while let Some(Ok(_)) = socket.next().await {}
}

async fn received_through_the_auth_failure<T>(
    rx: &mut mpsc::UnboundedReceiver<T>,
    kind_of: impl Fn(&T) -> &str,
) -> Vec<T> {
    let collect = async {
        let mut received = Vec::new();
        loop {
            let next = rx
                .recv()
                .await
                .expect("the publisher channel closed before the auth failure");
            let reached_auth_failure = kind_of(&next) == AUTH_FAILED_KIND;
            received.push(next);
            if reached_auth_failure {
                return received;
            }
        }
    };
    tokio::time::timeout(SUPERVISOR_BUDGET, collect)
        .await
        .expect("a rejected password never produced an auth failure")
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

    let events = received_through_the_auth_failure(&mut rx, |event| event.kind.as_str()).await;

    server.abort();
    drop(client);

    assert_eq!(
        events.last().map(|event| event.kind.as_str()),
        Some(AUTH_FAILED_KIND)
    );
}

struct StateProbePublisher {
    client: Arc<OnceLock<Arc<ObsClient>>>,
    tx: mpsc::UnboundedSender<(String, Option<ConnectionState>)>,
}

impl EventPublisher for StateProbePublisher {
    fn publish(&self, event: Event) {
        let state = self.client.get().map(|c| c.connection_state());
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.tx.send((event.kind, state));
    }
}

#[tokio::test]
async fn the_connection_state_is_already_settled_when_the_auth_failure_is_published() {
    let (listener, port) = bind_loopback().await;
    let (gate_tx, gate_rx) = oneshot::channel();
    let server = tokio::spawn(serve_one_gated_close_frame(
        listener,
        AUTHENTICATION_FAILED,
        "authentication failed",
        Some(gate_rx),
    ));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let slot: Arc<OnceLock<Arc<ObsClient>>> = Arc::new(OnceLock::new());

    let client = Arc::new(
        ObsClient::connect(
            &format!("127.0.0.1:{port}"),
            Some("wrong-password"),
            Arc::new(StateProbePublisher {
                client: Arc::clone(&slot),
                tx,
            }),
        )
        .await
        .unwrap(),
    );
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the slot is filled at most once"
    )]
    let _ = slot.set(Arc::clone(&client));
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = gate_tx.send(());

    let observed = received_through_the_auth_failure(&mut rx, |entry| entry.0.as_str()).await;

    server.abort();
    drop(client);

    for (kind, state_at_publish) in &observed {
        assert_eq!(
            *state_at_publish,
            Some(ConnectionState::Disconnected),
            "{kind} was published before the rejection was stored"
        );
    }
}
async fn serve_a_second_hello_instead_of_identified(listener: TcpListener) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let hello = r#"{"op":0,"d":{"obsWebSocketVersion":"5.5.0","rpcVersion":1}}"#;
    if socket.send(Message::text(hello)).await.is_err() {
        return;
    }
    while let Some(Ok(message)) = socket.next().await {
        if matches!(message, Message::Text(_)) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the client may already be gone"
            )]
            let _ = socket.send(Message::text(hello)).await;
            return;
        }
    }
}

async fn serve_plain_http_answer(listener: TcpListener) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Ok((mut stream, _)) = listener.accept().await else {
        return;
    };
    let mut request = [0_u8; 1024];
    if stream.read(&mut request).await.is_err() {
        return;
    }
    let answer = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the client may already be gone"
    )]
    let _ = stream.write_all(answer.as_bytes()).await;
}

async fn serve_accept_then_reset(listener: TcpListener) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    drop(stream);
}

struct Quiet;

impl EventPublisher for Quiet {
    fn publish(&self, _event: Event) {}
}

async fn recorded_failure_code(port: u16) -> String {
    use forge_platform_core::BuiltinStatus;
    let client = ObsClient::connect(
        &format!("127.0.0.1:{port}"),
        Some("wrong-password"),
        Arc::new(Quiet),
    )
    .await
    .unwrap();
    client.set_auto_reconnect(false);
    settle_until("a connect failure was recorded", || {
        client.connect_failure().is_some()
    })
    .await;
    client
        .connect_failure()
        .map(|failure| failure.code().name().to_owned())
        .unwrap()
}

#[tokio::test]
async fn each_way_a_connect_can_fail_records_its_own_user_facing_code() {
    for (label, expected) in [
        ("auth close code", "password_rejected"),
        ("other close code", "connect_failed"),
        ("hello where identified belongs", "password_rejected"),
        ("plain http answer", "connect_failed"),
        ("accept then reset", "unreachable"),
    ] {
        let (listener, port) = bind_loopback().await;
        let server = match label {
            "auth close code" => tokio::spawn(serve_one_close_frame(
                listener,
                AUTHENTICATION_FAILED,
                "authentication failed",
            )),
            "other close code" => tokio::spawn(serve_one_close_frame(
                listener,
                SESSION_INVALIDATED,
                "closed by the test peer",
            )),
            "hello where identified belongs" => {
                tokio::spawn(serve_a_second_hello_instead_of_identified(listener))
            }
            "plain http answer" => tokio::spawn(serve_plain_http_answer(listener)),
            _ => tokio::spawn(serve_accept_then_reset(listener)),
        };

        let code = recorded_failure_code(port).await;
        server.abort();

        assert_eq!(code, expected, "{label}");
    }
}
