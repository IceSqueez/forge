#![allow(clippy::unwrap_used, clippy::panic)]
use std::sync::{Arc, OnceLock};
use std::time::Duration;

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
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
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

    let deadline = std::time::Instant::now() + SUPERVISOR_BUDGET;
    let mut kinds: Vec<String> = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let Ok(received) = tokio::time::timeout(remaining, rx.recv()).await else {
            break;
        };
        let kind = received
            .expect("the publisher channel closed without an event")
            .kind;
        let reached_auth_failure = kind == AUTH_FAILED_KIND;
        kinds.push(kind);
        if reached_auth_failure {
            break;
        }
    }

    server.abort();
    drop(client);

    assert!(
        kinds.iter().any(|kind| kind == AUTH_FAILED_KIND),
        "a rejected password produced only {kinds:?}"
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

async fn serve_one_gated_close_frame(
    listener: TcpListener,
    code: u16,
    reason: &'static str,
    gate: oneshot::Receiver<()>,
) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    if gate.await.is_err() {
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

#[tokio::test]
async fn the_connection_state_is_already_settled_when_the_auth_failure_is_published() {
    let (listener, port) = bind_loopback().await;
    let (gate_tx, gate_rx) = oneshot::channel();
    let server = tokio::spawn(serve_one_gated_close_frame(
        listener,
        AUTHENTICATION_FAILED,
        "authentication failed",
        gate_rx,
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

    let deadline = std::time::Instant::now() + SUPERVISOR_BUDGET;
    let mut observed: Vec<(String, Option<ConnectionState>)> = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let Ok(received) = tokio::time::timeout(remaining, rx.recv()).await else {
            break;
        };
        let entry = received.expect("the publisher channel closed without an event");
        let reached_auth_failure = entry.0 == AUTH_FAILED_KIND;
        observed.push(entry);
        if reached_auth_failure {
            break;
        }
    }

    server.abort();
    drop(client);

    assert!(
        observed.iter().any(|(kind, _)| kind == AUTH_FAILED_KIND),
        "a rejected password produced only {observed:?}"
    );
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
    let started = std::time::Instant::now();
    loop {
        if let Some(failure) = client.connect_failure() {
            return failure.code().name().to_owned();
        }
        assert!(
            started.elapsed() < SUPERVISOR_BUDGET,
            "no connect failure was recorded"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
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
