use std::sync::Arc;

use forge_platform_kick::chat::PUSHER_APP_KEY;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};

use super::state::Shared;

pub(crate) const SOCKET_PATH_PREFIX: &str = "/app";

const OUTBOX_CAPACITY: usize = 256;
const ACTIVITY_TIMEOUT_SECS: u64 = 120;
const UNKNOWN_APP_CODE: u16 = 4001;

pub(crate) fn frame(event: &str, channel: Option<&str>, data: &Value) -> String {
    let mut frame = json!({ "event": event, "data": data.to_string() });
    if let Some(channel) = channel {
        frame["channel"] = Value::String(channel.to_owned());
    }
    frame.to_string()
}

pub(crate) async fn serve(
    listener: TcpListener,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    connections.spawn(connection(stream, Arc::clone(&shared), shutdown.clone()));
                }
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    connections.shutdown().await;
}

async fn connection(stream: TcpStream, shared: Arc<Shared>, mut shutdown: watch::Receiver<bool>) {
    let mut path = String::new();
    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, PathCapture(&mut path)).await
    else {
        return;
    };
    let app_key = path
        .strip_prefix(SOCKET_PATH_PREFIX)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or_default()
        .to_owned();
    if app_key != PUSHER_APP_KEY {
        shared.mutate(|inner| inner.open_session(app_key.clone(), None));
        let refusal = json!({
            "code": UNKNOWN_APP_CODE,
            "message": format!("App key {app_key} not in this cluster. Did you forget to specify the cluster?")
        });
        let _ = socket
            .send(Message::Text(frame("pusher:error", None, &refusal).into()))
            .await;
        let _ = socket.close(None).await;
        return;
    }

    let (outbox, mut frames_rx) = mpsc::channel::<String>(OUTBOX_CAPACITY);
    let session_id = shared.mutate(|inner| inner.open_session(app_key, Some(outbox)));
    let established = json!({
        "socket_id": format!("{session_id}.{session_id}"),
        "activity_timeout": ACTIVITY_TIMEOUT_SECS
    });
    let greeting = frame("pusher:connection_established", None, &established);
    if socket.send(Message::Text(greeting.into())).await.is_ok() {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                outgoing = frames_rx.recv() => {
                    let Some(outgoing) = outgoing else { break };
                    if socket.send(Message::Text(outgoing.into())).await.is_err() {
                        break;
                    }
                }
                incoming = socket.next() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let Some(reply) = answer(&shared, session_id, &text) else { continue };
                        if socket.send(Message::Text(reply.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
    shared.mutate(|inner| inner.close_session(session_id));
    let _ = socket.close(None).await;
}

fn answer(shared: &Shared, session_id: u64, text: &str) -> Option<String> {
    let request: Value = serde_json::from_str(text).ok()?;
    let channel = request
        .pointer("/data/channel")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match request.get("event").and_then(Value::as_str)? {
        "pusher:ping" => Some(frame("pusher:pong", None, &json!({}))),
        "pusher:subscribe" => {
            let channel = channel?;
            shared.mutate(|inner| inner.subscribe(session_id, &channel));
            Some(frame(
                "pusher_internal:subscription_succeeded",
                Some(&channel),
                &json!({}),
            ))
        }
        "pusher:unsubscribe" => {
            let channel = channel?;
            shared.mutate(|inner| inner.unsubscribe(session_id, &channel));
            None
        }
        _ => None,
    }
}

struct PathCapture<'a>(&'a mut String);

impl Callback for PathCapture<'_> {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        *self.0 = request.uri().path().to_owned();
        Ok(response)
    }
}
