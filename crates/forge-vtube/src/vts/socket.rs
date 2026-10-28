use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::error::VTubeError;
use crate::vts::protocol::new_request;

pub(crate) type VtsWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(crate) enum Inbound {
    Closed,
    Envelope(Value),
    Skipped,
}

pub(crate) async fn dial(endpoint: &str) -> Result<VtsWs, String> {
    tokio_tungstenite::connect_async(endpoint)
        .await
        .map(|(ws, _)| ws)
        .map_err(|e| e.to_string())
}

pub(crate) async fn send_text(ws: &mut VtsWs, text: String) -> Result<(), VTubeError> {
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| VTubeError::Connect(e.to_string()))
}

pub(crate) async fn send_ws_msg<T: serde::Serialize>(
    ws: &mut VtsWs,
    msg: &T,
) -> Result<(), VTubeError> {
    let text = serde_json::to_string(msg).map_err(VTubeError::Json)?;
    send_text(ws, text).await
}

pub(crate) async fn next_inbound(ws: &mut VtsWs) -> Inbound {
    decode_inbound(ws.next().await)
}

fn decode_inbound(frame: Option<Result<Message, WsError>>) -> Inbound {
    match frame {
        None | Some(Err(_)) => Inbound::Closed,
        Some(Ok(Message::Text(text))) => {
            serde_json::from_str::<Value>(&text).map_or(Inbound::Skipped, Inbound::Envelope)
        }
        Some(Ok(_)) => Inbound::Skipped,
    }
}

pub(crate) async fn recv_next_text(ws: &mut VtsWs) -> Result<Value, VTubeError> {
    loop {
        match ws.next().await {
            None => return Err(VTubeError::Connect("connection closed".to_owned())),
            Some(Err(e)) => return Err(VTubeError::Connect(e.to_string())),
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(VTubeError::Json);
            }
            Some(Ok(_)) => {}
        }
    }
}

async fn recv_reply(ws: &mut VtsWs, request_id: &str) -> Result<Value, VTubeError> {
    loop {
        let msg = recv_next_text(ws).await?;
        if msg["requestID"].as_str() == Some(request_id) {
            return Ok(msg);
        }
    }
}

pub(crate) async fn exchange(
    ws: &mut VtsWs,
    msg_type: &str,
    data: Value,
    limit: Duration,
) -> Result<Value, VTubeError> {
    let req = new_request(msg_type, data);
    send_ws_msg(ws, &req).await?;
    tokio::time::timeout(limit, recv_reply(ws, &req.request_id))
        .await
        .map_err(|_| VTubeError::Timeout)?
}
