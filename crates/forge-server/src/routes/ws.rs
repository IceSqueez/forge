use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use tokio::sync::broadcast::error::RecvError;

use crate::bus_adapter::{ClientFilterSet, WsFrame};
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
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    upgrade.on_upgrade(move |socket| handle_socket(socket, state, addr, user_agent))
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

    let ctx = DispatchContext {
        bus: Arc::clone(&state.bus),
        bus_adapter: Arc::clone(&state.bus_adapter),
        dp: Arc::clone(&state.dp),
        auth_state: Arc::clone(&state.auth),
        client: Arc::clone(&client),
        auth_required_for_reads: state.auth.auth_required_for_reads,
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
                        if socket
                            .send(Message::Text(response_json.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
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
                        client.record_event();
                    }
                    Ok(WsFrame::Close) | Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(n)) => {
                        client.drop_counter.fetch_add(n, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    state.bus_adapter.unregister_client(handle.id).await;
}
