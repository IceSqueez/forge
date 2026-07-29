use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Json;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::sync::broadcast::error::RecvError;

use crate::bus_adapter::{ClientFilterSet, WsFrame};
use crate::origin::is_origin_allowed;
use crate::protocol::{
    DispatchContext, WsEnvelope, WsRequest, WsResponse, dispatch, serialize_response_frame,
};
use crate::server::AppState;
use crate::ws_client::{WsClient, detect_from_user_agent};

pub async fn ws_handler(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if !is_origin_allowed(&state.allowed_origins, origin) {
        return origin_rejected_response();
    }

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    upgrade.on_upgrade(move |socket| handle_socket(socket, state, addr, user_agent))
}

fn origin_rejected_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": {
                "code": "ORIGIN_NOT_ALLOWED",
                "message": "Origin not allowed"
            }
        })),
    )
        .into_response()
}

async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    addr: SocketAddr,
    user_agent: Option<String>,
) {
    let (handle, mut event_rx) = state
        .bus_adapter
        .register_client(ClientFilterSet::new(HashSet::new()))
        .await;

    let client = Arc::new(WsClient::new(
        handle.id,
        addr,
        Arc::clone(&handle.drop_counter),
    ));

    if let Some(ua) = &user_agent {
        let client_type = detect_from_user_agent(Some(ua.as_str()));
        client.client_type.store(Arc::new(client_type));
    }

    state
        .server_info
        .register(client.id, Arc::clone(&client))
        .await;

    let ctx = DispatchContext {
        bus: Arc::clone(&state.bus),
        bus_adapter: Arc::clone(&state.bus_adapter),
        actions: Arc::clone(&state.actions),
        globals: Arc::clone(&state.globals),
        user_globals: Arc::clone(&state.user_globals),
        auth_state: Arc::clone(&state.auth),
        client: Arc::clone(&client),
        auth_required_for_reads: state.auth.auth_required_for_reads,
        credentials: Arc::clone(&state.credentials),
        server_info: Arc::clone(&state.server_info),
        action_engine: Arc::clone(&state.action_engine),
        overlay_root: Arc::clone(&state.overlay_root),
    };

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    _ => break,
                };
                match msg {
                    Message::Text(text) => {
                        let response_json =
                            match serde_json::from_str::<WsEnvelope<WsRequest>>(&text) {
                                Ok(req) => {
                                    let resp = dispatch(req, &ctx).await;
                                    serialize_response_frame(&resp).to_string()
                                }
                                Err(e) => {
                                    let err_env = WsEnvelope {
                                        id: None,
                                        inner: WsResponse::Error {
                                            code: Some("INVALID_PAYLOAD".to_owned()),
                                            message: e.to_string(),
                                        },
                                    };
                                    serialize_response_frame(&err_env).to_string()
                                }
                            };
                        let response_bytes = response_json.len() as u64;
                        if socket
                            .send(Message::Text(response_json.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        client.bytes_sent_session.fetch_add(response_bytes, Ordering::Relaxed);
                        state.server_info.bandwidth.record(response_bytes);
                    }
                    Message::Binary(_) => {
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: 1003,
                                reason: "binary frames not supported".into(),
                            })))
                            .await;
                        break;
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }

            event = event_rx.recv() => {
                match event {
                    Ok(WsFrame::Text(json)) => {
                        let len = json.len() as u64;
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                        client.bytes_sent_session.fetch_add(len, Ordering::Relaxed);
                        state.server_info.bandwidth.record(len);
                        state.server_info.record_event_out();
                        client.record_event();
                    }
                    Ok(WsFrame::Close) => {
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(n)) => {
                        client.drop_counter.fetch_add(n, Ordering::Relaxed);
                        state.server_info.record_dropped_events(n);
                        let notice = dropped_notification(n);
                        let len = notice.len() as u64;
                        if socket.send(Message::Text(notice.into())).await.is_err() {
                            break;
                        }
                        client.bytes_sent_session.fetch_add(len, Ordering::Relaxed);
                        state.server_info.bandwidth.record(len);
                    }
                }
            }
        }
    }

    state.bus_adapter.unregister_client(handle.id).await;
    state.server_info.unregister(handle.id).await;
}

fn dropped_notification(n: u64) -> String {
    serde_json::json!({ "dropped": n }).to_string()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::dropped_notification;

    #[test]
    fn dropped_notification_serializes_count() {
        for n in [0_u64, 42] {
            let frame = dropped_notification(n);
            let parsed: serde_json::Value = serde_json::from_str(&frame).expect("valid json");
            assert_eq!(parsed["dropped"], n);
        }
    }
}
