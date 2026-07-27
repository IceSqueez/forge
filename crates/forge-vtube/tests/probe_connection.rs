#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use forge_vtube::{VTubeError, probe_connection};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

async fn bind_loopback() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A loopback peer that answers the first text frame with `reply` verbatim, forwarding what it
/// received so a test can assert on the request the probe actually sent.
async fn serve_one_reply(
    listener: TcpListener,
    reply: String,
    seen: Option<oneshot::Sender<String>>,
) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let Ok(Some(Ok(Message::Text(text)))) =
        tokio::time::timeout(Duration::from_secs(3), socket.next()).await
    else {
        return;
    };
    if let Some(tx) = seen {
        let _ = tx.send(text.to_string());
    }
    let _ = socket.send(Message::Text(reply.into())).await;
    while let Some(Ok(_)) = socket.next().await {}
}

/// Completes the web-socket upgrade and then goes silent, which is how a non-VTS server on the
/// probed port behaves: the handshake succeeds and no answer ever arrives.
async fn serve_silence(listener: TcpListener) {
    let Ok((stream, _)) = listener.accept().await else {
        return;
    };
    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    while let Some(Ok(_)) = socket.next().await {}
}

async fn probe_against(
    reply: serde_json::Value,
) -> Result<forge_vtube::VTubeProbeResult, VTubeError> {
    let (listener, port) = bind_loopback().await;
    let server = tokio::spawn(serve_one_reply(listener, reply.to_string(), None));
    let outcome = probe_connection("127.0.0.1", port).await;
    server.abort();
    outcome
}

/// `VTubeProbeResult` carries no `Debug`, so the usual `unwrap_err` is unavailable here.
fn expect_error(outcome: Result<forge_vtube::VTubeProbeResult, VTubeError>) -> VTubeError {
    match outcome {
        Err(e) => e,
        Ok(_) => panic!("the probe reported success against a peer that never served VTS state"),
    }
}

fn api_state_response(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "apiName": "VTubeStudioPublicAPI",
        "apiVersion": "1.0",
        "requestID": "probe",
        "messageType": "APIStateResponse",
        "data": data,
    })
}

#[tokio::test]
async fn the_probe_reads_the_version_and_session_flag_out_of_the_api_state_response() {
    let result = probe_against(api_state_response(serde_json::json!({
        "active": true,
        "vTubeStudioVersion": "1.30.1",
        "currentSessionAuthenticated": true,
    })))
    .await
    .unwrap();

    assert_eq!(result.api_version, "1.30.1");
    assert!(result.already_authenticated);
}

#[tokio::test]
async fn the_probe_pings_with_an_api_state_request() {
    let (listener, port) = bind_loopback().await;
    let (seen_tx, seen_rx) = oneshot::channel();
    let server = tokio::spawn(serve_one_reply(
        listener,
        api_state_response(serde_json::json!({})).to_string(),
        Some(seen_tx),
    ));

    let _ = probe_connection("127.0.0.1", port).await;

    let sent: serde_json::Value = serde_json::from_str(&seen_rx.await.unwrap()).unwrap();
    server.abort();
    assert_eq!(sent["messageType"], "APIStateRequest");
}

// Why: the probe runs against a port the user typed, so the peer answering may be any web-socket
// server. A thin response has to degrade into a reported version, never into a failed probe.
#[tokio::test]
async fn a_response_without_the_version_fields_degrades_instead_of_failing() {
    let result = probe_against(api_state_response(serde_json::json!({})))
        .await
        .unwrap();

    assert_eq!(result.api_version, "unknown");
    assert!(!result.already_authenticated);
}

#[tokio::test]
async fn a_reply_of_another_message_type_is_rejected_and_names_what_arrived() {
    let error = expect_error(
        probe_against(serde_json::json!({
            "messageType": "APIError",
            "data": { "errorID": 1, "message": "unknown request" },
        }))
        .await,
    );

    let VTubeError::Request { message } = &error else {
        panic!("expected a request failure, got {error:?}");
    };
    assert!(
        message.contains("APIError"),
        "the received message type was dropped from: {message}"
    );
}

#[tokio::test]
async fn a_peer_that_answers_with_non_json_text_is_rejected_without_panicking() {
    let (listener, port) = bind_loopback().await;
    let server = tokio::spawn(serve_one_reply(
        listener,
        "not json at all".to_owned(),
        None,
    ));

    let error = expect_error(probe_connection("127.0.0.1", port).await);

    server.abort();
    assert!(
        matches!(error, VTubeError::Json(_)),
        "expected a deserialization failure, got {error:?}"
    );
}

// Why: a refused port is the everyday case of VTube Studio not running. Collapsing it into the
// probe's timeout would hide the cause and stall the setup screen for the full five seconds.
#[tokio::test]
async fn a_refused_port_reports_the_connect_failure_rather_than_waiting_out_the_timeout() {
    let (listener, port) = bind_loopback().await;
    drop(listener);

    let error = expect_error(probe_connection("127.0.0.1", port).await);

    assert!(
        matches!(error, VTubeError::Connect(_)),
        "expected a connect failure, got {error:?}"
    );
}

// Why: a half-open peer never answers, and the probe is awaited by the setup screen. Without the
// deadline that screen would hang for as long as the peer keeps the socket open.
#[tokio::test]
async fn a_peer_that_never_answers_ends_in_a_timeout_instead_of_hanging() {
    let (listener, port) = bind_loopback().await;
    let server = tokio::spawn(serve_silence(listener));

    let error = expect_error(probe_connection("127.0.0.1", port).await);

    server.abort();
    assert!(
        matches!(error, VTubeError::Timeout),
        "expected the probe deadline to fire, got {error:?}"
    );
}
