use std::collections::HashSet;
use std::net::SocketAddr;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Extension;
use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

use crate::access::auth::AuthState;
use crate::access::origin::{accepts_origin, is_well_formed_origin};
use crate::delivery::bus_adapter::{ClientFilterSet, WsFrame, dropped_notification};
use crate::delivery::ws_client::{WsClient, detect_from_user_agent};
use crate::lifecycle::listener::PeerInfo;
use crate::lifecycle::server::AppState;
use crate::protocol::{
    DispatchContext, Session, WsEnvelope, WsErrorCode, WsRequest, WsResponse, dispatch,
    serialize_response_frame,
};
use crate::routes::refusal::{RefusalCode, refusal_response};

const HEADER_LOG_PLACEHOLDER: &str = "<absent-or-invalid>";
pub(crate) const MAX_MESSAGE_BYTES: usize = 256 * 1024;
pub(crate) const PRE_AUTH_MAX_MESSAGE_BYTES: usize = 16 * 1024;
pub(crate) const PRE_AUTH_WINDOW: Duration = Duration::from_secs(10);

pub(crate) async fn ws_handler(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(shutdown): Extension<watch::Receiver<bool>>,
    ConnectInfo(peer): ConnectInfo<PeerInfo>,
    headers: HeaderMap,
) -> Response {
    let origin_header = headers.get(header::ORIGIN);
    let origin = origin_header.and_then(|v| v.to_str().ok());
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if (origin_header.is_some() && origin.is_none())
        || !accepts_origin(&state.allowed_origins, origin, host)
    {
        let origin_log = origin_log_value(origin);
        let host_log = host.unwrap_or(HEADER_LOG_PLACEHOLDER);
        tracing::warn!(
            origin = origin_log,
            host = host_log,
            "rejected overlay websocket origin; add it in Settings or open the page by IP address"
        );
        return origin_rejected_response();
    }

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            peer.mark_upgraded();
            handle_socket(socket, state, shutdown, peer.addr, user_agent).await;
        })
}

fn origin_log_value(origin: Option<&str>) -> &str {
    match origin {
        Some(value) if is_well_formed_origin(value) => value,
        _ => HEADER_LOG_PLACEHOLDER,
    }
}

fn origin_rejected_response() -> Response {
    refusal_response(
        StatusCode::FORBIDDEN,
        RefusalCode::OriginNotAllowed,
        "Origin not allowed",
    )
}

const CLOSE_REASON_REVOKED: &str = "authentication revoked";
const CLOSE_REASON_PRE_AUTH: &str = "no authentication within the pre-auth window";
const CLOSE_REASON_TOO_LARGE: &str = "message too large";
const CLOSE_REASON_BINARY: &str = "binary frames not supported";

struct Connection {
    socket: WebSocket,
    state: AppState,
    ctx: DispatchContext,
    event_rx: broadcast::Receiver<WsFrame>,
    overlay_session: bool,
    has_sent_any_message: bool,
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    shutdown: watch::Receiver<bool>,
    addr: SocketAddr,
    user_agent: Option<String>,
) {
    let (handle, event_rx) = state
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
        .bus_adapter
        .attach_session(client.id, Arc::clone(&client))
        .await;

    let ctx = DispatchContext {
        services: state.services(),
        session: Session::new(client),
    };
    let mut connection = Connection {
        socket,
        state,
        ctx,
        event_rx,
        overlay_session: false,
        has_sent_any_message: false,
    };
    connection.run(shutdown).await;
    connection
        .state
        .bus_adapter
        .unregister_client(handle.id)
        .await;
}

impl Connection {
    fn client(&self) -> &Arc<WsClient> {
        &self.ctx.session.client
    }

    async fn run(&mut self, mut shutdown: watch::Receiver<bool>) {
        let mut policy_changes = self.state.auth.policy_changes();
        let mut pre_auth_window_open = true;
        let pre_auth_deadline = tokio::time::sleep(self.state.timing.pre_auth_window);
        tokio::pin!(pre_auth_deadline);

        loop {
            let flow = tokio::select! {
                () = shutdown_signalled(&mut shutdown) => {
                    self.close(None).await;
                    ControlFlow::Break(())
                }
                Ok(()) = policy_changes.changed() => self.on_policy_change().await,
                () = &mut pre_auth_deadline, if pre_auth_window_open => {
                    pre_auth_window_open = false;
                    self.on_pre_auth_deadline().await
                }
                incoming = self.socket.recv() => self.on_incoming(incoming).await,
                event = self.event_rx.recv() => self.on_bus_event(event).await,
            };
            if flow.is_break() {
                break;
            }
        }
    }

    #[expect(
        clippy::let_underscore_must_use,
        reason = "the peer may already be gone; the session ends either way"
    )]
    async fn close(&mut self, frame: Option<CloseFrame>) {
        let _ = self.socket.send(Message::Close(frame)).await;
    }

    async fn close_with(&mut self, code: u16, reason: &'static str) -> ControlFlow<()> {
        self.close(Some(CloseFrame {
            code,
            reason: reason.into(),
        }))
        .await;
        ControlFlow::Break(())
    }

    async fn on_policy_change(&mut self) -> ControlFlow<()> {
        let admitted = self
            .state
            .auth
            .admits_session(self.client().bearer_generation(), self.overlay_session);
        if admitted {
            ControlFlow::Continue(())
        } else {
            self.close_with(close_code::POLICY, CLOSE_REASON_REVOKED)
                .await
        }
    }

    async fn on_pre_auth_deadline(&mut self) -> ControlFlow<()> {
        let authenticated = self.client().bearer_generation().is_some() || self.overlay_session;
        let stays_open =
            authenticated || (!self.state.auth.reads_required() && self.has_sent_any_message);
        if stays_open {
            ControlFlow::Continue(())
        } else {
            self.close_with(close_code::POLICY, CLOSE_REASON_PRE_AUTH)
                .await
        }
    }

    async fn on_incoming(
        &mut self,
        incoming: Option<Result<Message, axum::Error>>,
    ) -> ControlFlow<()> {
        let Some(Ok(message)) = incoming else {
            return ControlFlow::Break(());
        };
        self.has_sent_any_message = true;
        match message {
            Message::Text(text) => self.on_text(text.as_str()).await,
            Message::Binary(_) => {
                self.close_with(close_code::UNSUPPORTED, CLOSE_REASON_BINARY)
                    .await
            }
            Message::Close(_) => ControlFlow::Break(()),
            Message::Ping(_) | Message::Pong(_) => ControlFlow::Continue(()),
        }
    }

    async fn on_text(&mut self, text: &str) -> ControlFlow<()> {
        if text.len() > message_limit(self.client(), &self.state.auth) {
            return self
                .close_with(close_code::SIZE, CLOSE_REASON_TOO_LARGE)
                .await;
        }
        let response = match serde_json::from_str::<WsEnvelope<WsRequest>>(text) {
            Ok(request) => dispatch(request, &self.ctx).await,
            Err(error) => WsEnvelope {
                id: None,
                inner: WsResponse::error(WsErrorCode::InvalidPayload, error.to_string()),
            },
        };
        let response_json = serialize_response_frame(&response).to_string();
        let promoted = self.ctx.session.overlay_channel_swap.lock().await.take();
        if let Some(promoted) = promoted {
            self.event_rx = promoted;
            self.overlay_session = true;
        }
        self.send_counted(response_json).await?;
        if self
            .ctx
            .session
            .close_after_auth_failure
            .swap(false, Ordering::SeqCst)
        {
            self.close(None).await;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    async fn on_bus_event(&mut self, event: Result<WsFrame, RecvError>) -> ControlFlow<()> {
        match event {
            Ok(WsFrame::Text(json)) => {
                self.send_counted(json).await?;
                self.state.server_info.record_event_out();
                self.client().record_event();
                ControlFlow::Continue(())
            }
            Ok(WsFrame::Close) => {
                self.close(None).await;
                ControlFlow::Break(())
            }
            Err(RecvError::Closed) => ControlFlow::Break(()),
            Err(RecvError::Lagged(skipped)) => {
                self.client()
                    .drop_counter
                    .fetch_add(skipped, Ordering::Relaxed);
                self.state.server_info.record_dropped_events(skipped);
                self.send_counted(dropped_notification(skipped)).await
            }
        }
    }

    async fn send_counted(&mut self, text: String) -> ControlFlow<()> {
        let bytes = text.len() as u64;
        if self.socket.send(Message::Text(text.into())).await.is_err() {
            return ControlFlow::Break(());
        }
        self.client()
            .bytes_sent_session
            .fetch_add(bytes, Ordering::Relaxed);
        self.state.server_info.bandwidth.record(bytes);
        ControlFlow::Continue(())
    }
}
pub(crate) async fn shutdown_signalled(shutdown: &mut watch::Receiver<bool>) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a dropped shutdown sender also means stopping"
    )]
    let _ = shutdown.wait_for(|stopping| *stopping).await;
}

fn message_limit(client: &WsClient, auth: &AuthState) -> usize {
    let bearer_current = client
        .bearer_generation()
        .is_some_and(|generation| generation == auth.token_generation());
    if bearer_current {
        MAX_MESSAGE_BYTES
    } else {
        PRE_AUTH_MAX_MESSAGE_BYTES
    }
}
