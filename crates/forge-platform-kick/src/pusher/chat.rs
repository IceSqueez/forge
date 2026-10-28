use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_net::Backoff;
use forge_platform_core::chat::ConnectionState;
use forge_platform_core::{ConnectionTransition, EndpointSurface, PlatformEndpoints};
use forge_types::ChatPayload;
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ban_ledger::USER_BANNED_EVENT_KIND;
use crate::pusher::chatroom_lookup::ChannelInfoFetcher;
use crate::pusher::error::KickError;
use crate::pusher::normalize;

pub const PUSHER_APP_KEY: &str = "32cbd69e4b950bf97679";

const PUSHER_PROTOCOL_PARAMS: &str = "protocol=7&client=js&version=7.6.0&flash=false";

const PING_INTERVAL: Duration = Duration::from_secs(30);

const PUSHER_RECONNECT_IMMEDIATELY_CODES: std::ops::RangeInclusive<u16> = 4200..=4299;

const SESSION_STOP_GRACE: Duration = Duration::from_secs(5);

const PUSHER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

type PusherSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChatLinkStatus {
    pub(crate) state: ConnectionState,
    pub(crate) transition: ConnectionTransition,
}

impl ChatLinkStatus {
    fn expected(state: ConnectionState) -> Self {
        Self {
            state,
            transition: ConnectionTransition::Expected,
        }
    }

    fn unexpected(state: ConnectionState) -> Self {
        Self {
            state,
            transition: ConnectionTransition::Unexpected,
        }
    }
}

fn is_server_requested_reconnect(frame: Option<&CloseFrame>) -> bool {
    frame.is_some_and(|frame| PUSHER_RECONNECT_IMMEDIATELY_CODES.contains(&u16::from(frame.code)))
}

pub struct KickChat {
    fetcher: Arc<ChannelInfoFetcher>,
    slug: String,
    ws_url: String,
}

pub struct KickChatHandle {
    cancel: CancellationToken,
    tasks: JoinSet<()>,
    state_rx: watch::Receiver<ChatLinkStatus>,
}

impl KickChatHandle {
    pub fn connection_state(&self) -> ConnectionState {
        self.state_rx.borrow().state
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ChatLinkStatus> {
        self.state_rx.clone()
    }

    pub(crate) fn attach<F>(&mut self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(task);
    }

    pub fn shutdown(mut self) {
        self.cancel.cancel();
        self.tasks.detach_all();
    }

    pub async fn stop(mut self) {
        self.cancel.cancel();
        if tokio::time::timeout(SESSION_STOP_GRACE, join_all(&mut self.tasks))
            .await
            .is_err()
        {
            warn!("kick chat session tasks outlived the stop grace period; aborting");
            self.tasks.abort_all();
            join_all(&mut self.tasks).await;
        }
    }
}

async fn join_all(tasks: &mut JoinSet<()>) {
    while let Some(joined) = tasks.join_next().await {
        if let Err(error) = joined
            && error.is_panic()
        {
            warn!("kick chat session task panicked");
        }
    }
}

impl KickChat {
    pub fn new(
        endpoints: &PlatformEndpoints,
        slug: String,
        channel_info: Arc<ChannelInfoFetcher>,
    ) -> Self {
        Self {
            fetcher: channel_info,
            slug,
            ws_url: pusher_ws_url(endpoints.base_url(EndpointSurface::KickChatSocket)),
        }
    }

    pub async fn connect(self, event_tx: mpsc::Sender<Event>) -> Result<KickChatHandle, KickError> {
        self.connect_until(event_tx, CancellationToken::new()).await
    }

    pub(crate) async fn connect_until(
        self,
        event_tx: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<KickChatHandle, KickError> {
        let Self {
            fetcher,
            slug,
            ws_url,
        } = self;
        let channel_info = cancel
            .run_until_cancelled(fetcher.fetch(&slug))
            .await
            .ok_or(KickError::ConnectCancelled)??;
        let ws_stream = cancel
            .run_until_cancelled(connect_ws(&ws_url))
            .await
            .ok_or(KickError::ConnectCancelled)??;

        let (state_tx, state_rx) =
            watch::channel(ChatLinkStatus::unexpected(ConnectionState::Connecting));

        let mut tasks = JoinSet::new();
        tasks.spawn(run_loop(
            ws_stream,
            channel_info.chatroom_id,
            RunLoopContext {
                event_tx,
                cancel: cancel.clone(),
                ws_url,
                fetcher,
                slug,
                state_tx,
            },
        ));

        Ok(KickChatHandle {
            cancel,
            tasks,
            state_rx,
        })
    }
}

fn pusher_ws_url(ws_base: &str) -> String {
    format!("{ws_base}/{PUSHER_APP_KEY}?{PUSHER_PROTOCOL_PARAMS}")
}

async fn connect_ws(url: &str) -> Result<PusherSocket, KickError> {
    let (ws_stream, _) = tokio::time::timeout(
        PUSHER_CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async(url),
    )
    .await
    .map_err(|_| KickError::WebSocket {
        reason: format!(
            "connect timed out after {}s",
            PUSHER_CONNECT_TIMEOUT.as_secs()
        ),
    })?
    .map_err(|e| KickError::WebSocket {
        reason: e.to_string(),
    })?;
    Ok(ws_stream)
}

async fn send_subscribe(ws: &mut PusherSocket, chatroom_id: u64) -> Result<(), KickError> {
    use futures::SinkExt;
    let frame = serde_json::json!({
        "event": "pusher:subscribe",
        "data": { "channel": format!("chatrooms.{chatroom_id}.v2") }
    });
    let text = serde_json::to_string(&frame).map_err(|e| KickError::WebSocket {
        reason: format!("serialize subscribe: {e}"),
    })?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| KickError::WebSocket {
            reason: e.to_string(),
        })
}

async fn send_ping(ws: &mut PusherSocket) -> Result<(), KickError> {
    use futures::SinkExt;
    let frame = serde_json::json!({"event": "pusher:ping", "data": {}});
    let text = serde_json::to_string(&frame).map_err(|e| KickError::WebSocket {
        reason: format!("serialize ping: {e}"),
    })?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| KickError::WebSocket {
            reason: e.to_string(),
        })
}

struct RunLoopContext {
    event_tx: mpsc::Sender<Event>,
    cancel: CancellationToken,
    ws_url: String,
    fetcher: Arc<ChannelInfoFetcher>,
    slug: String,
    state_tx: watch::Sender<ChatLinkStatus>,
}

enum SessionEnd {
    Closed,
    Lost(ConnectionTransition),
}

fn channel_info_failure(err: &KickError) -> String {
    match err {
        KickError::Http { status, .. } => format!("HTTP {status}"),
        KickError::Network { reason } | KickError::WebSocket { reason } => reason.clone(),
        KickError::ChannelInfoUnavailable { reason, .. } => {
            format!("channel info request failed: {reason}")
        }
        KickError::ChatroomIdNotFound { .. } => "chatroom id missing".to_owned(),
        KickError::ConnectCancelled => err.to_string(),
    }
}

async fn run_loop(ws_stream: PusherSocket, chatroom_id: u64, ctx: RunLoopContext) {
    let mut backoff = Backoff::default();
    let mut link = Some((ws_stream, chatroom_id));

    while let Some((mut ws_stream, chatroom_id)) = link {
        let SessionEnd::Lost(transition) =
            run_session(&mut ws_stream, chatroom_id, &ctx, &mut backoff).await
        else {
            break;
        };
        ctx.state_tx.send_replace(ChatLinkStatus {
            state: ConnectionState::Reconnecting,
            transition,
        });
        link = reconnect(&ctx, &mut backoff).await;
    }

    info!("kick chat close requested");
    ctx.state_tx
        .send_replace(ChatLinkStatus::expected(ConnectionState::Disconnected));
}

async fn run_session(
    ws_stream: &mut PusherSocket,
    chatroom_id: u64,
    ctx: &RunLoopContext,
    backoff: &mut Backoff,
) -> SessionEnd {
    match ctx
        .cancel
        .run_until_cancelled(send_subscribe(ws_stream, chatroom_id))
        .await
    {
        None => return SessionEnd::Closed,
        Some(Err(e)) => warn!(error = %e, "subscribe send failed"),
        Some(Ok(())) => {}
    }

    ctx.state_tx
        .send_replace(ChatLinkStatus::unexpected(ConnectionState::Connected));
    info!(chatroom_id, "kick chat connected");

    let mut ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;
    let mut session_healthy = false;

    loop {
        tokio::select! {
            biased;

            () = ctx.cancel.cancelled() => return SessionEnd::Closed,

            () = tokio::time::sleep_until(ping_deadline) => {
                match ctx.cancel.run_until_cancelled(send_ping(ws_stream)).await {
                    None => return SessionEnd::Closed,
                    Some(Err(e)) => {
                        warn!(error = %e, "ping failed; reconnecting");
                        return SessionEnd::Lost(ConnectionTransition::Unexpected);
                    }
                    Some(Ok(())) => ping_deadline = tokio::time::Instant::now() + PING_INTERVAL,
                }
            }

            msg = ws_stream.next() => match msg {
                None | Some(Err(_)) => {
                    warn!("kick chat WebSocket closed; reconnecting");
                    return SessionEnd::Lost(ConnectionTransition::Unexpected);
                }
                Some(Ok(Message::Text(text))) => {
                    let Some(health) = ctx
                        .cancel
                        .run_until_cancelled(handle_ws_text(&text, &ctx.event_tx))
                        .await
                    else {
                        return SessionEnd::Closed;
                    };
                    if health == WsFrameHealth::Healthy && !session_healthy {
                        session_healthy = true;
                        backoff.reset();
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    return SessionEnd::Lost(server_close_transition(frame.as_ref()));
                }
                Some(Ok(_)) => {}
            }
        }
    }
}

fn server_close_transition(frame: Option<&CloseFrame>) -> ConnectionTransition {
    match frame {
        Some(frame) => warn!(
            code = %frame.code,
            reason = %frame.reason,
            "kick chat server sent close frame; reconnecting",
        ),
        None => warn!("kick chat server sent close frame; reconnecting"),
    }
    if is_server_requested_reconnect(frame) {
        ConnectionTransition::Expected
    } else {
        ConnectionTransition::Unexpected
    }
}

async fn reconnect(ctx: &RunLoopContext, backoff: &mut Backoff) -> Option<(PusherSocket, u64)> {
    loop {
        ctx.cancel
            .run_until_cancelled(tokio::time::sleep(backoff.next_delay()))
            .await?;

        let chatroom_id = match ctx
            .cancel
            .run_until_cancelled(ctx.fetcher.fetch(&ctx.slug))
            .await?
        {
            Ok(info) => info.chatroom_id,
            Err(e) => {
                warn!(
                    error = %channel_info_failure(&e),
                    "channel info fetch failed on reconnect"
                );
                continue;
            }
        };

        match ctx
            .cancel
            .run_until_cancelled(connect_ws(&ctx.ws_url))
            .await?
        {
            Ok(ws_stream) => {
                debug!(chatroom_id, "kick chat reconnected");
                return Some((ws_stream, chatroom_id));
            }
            Err(_) => warn!("kick chat WS reconnect failed"),
        }
    }
}
async fn handle_ws_text(raw: &str, event_tx: &mpsc::Sender<Event>) -> WsFrameHealth {
    let frame: PusherFrame = match serde_json::from_str(raw) {
        Ok(f) => f,
        Err(e) => {
            debug!(error = %e, "unparseable Pusher frame; skipping");
            return WsFrameHealth::Healthy;
        }
    };

    let event_name = frame.event.as_str();

    if event_name == "pusher:error" {
        match pusher_error_code(&frame.data) {
            Some(code) => warn!(code, frame_len = raw.len(), "kick chat pusher error frame"),
            None => warn!(frame_len = raw.len(), "kick chat pusher error frame"),
        }
        return WsFrameHealth::Error;
    }

    if event_name == "pusher:pong" || event_name.starts_with("pusher_internal:") {
        return WsFrameHealth::Healthy;
    }

    let payload_str = match frame.data.as_str() {
        Some(s) => s.to_owned(),
        None => {
            debug!(event = %event_name, "Pusher frame data is not a string; skipping");
            return WsFrameHealth::Healthy;
        }
    };

    let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            debug!(error = %e, event = %event_name, "failed to parse event data; skipping");
            return WsFrameHealth::Healthy;
        }
    };

    if let Some(event) = build_event(event_name, payload)
        && event_tx.send(event).await.is_err()
    {
        debug!("kick chat event receiver dropped");
    }
    WsFrameHealth::Healthy
}

fn pusher_error_code(data: &serde_json::Value) -> Option<i64> {
    let decoded;
    let body = match data.as_str() {
        Some(text) => {
            decoded = serde_json::from_str::<serde_json::Value>(text).ok()?;
            &decoded
        }
        None => data,
    };
    let code = body.get("code")?;
    code.as_i64().or_else(|| code.as_str()?.parse().ok())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WsFrameHealth {
    Healthy,
    Error,
}

pub(crate) fn build_event(event_name: &str, payload: serde_json::Value) -> Option<Event> {
    let (kind, normalized) = match event_name {
        "App\\Events\\ChatMessageEvent" => {
            let mut normalized = normalize::chat_message_sent(&payload);
            let chat_payload = normalize::chat_message_chat_payload(&payload);
            normalized[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
            if let Some(viewer) = normalize::chat_message_viewer(&payload) {
                viewer.attach(&mut normalized);
            }
            ("kick.chat.message.sent", normalized)
        }
        "App\\Events\\MessageDeletedEvent" => (
            "kick.chat.message.deleted",
            normalize::chat_message_deleted(&payload),
        ),
        "App\\Events\\UserBannedEvent" => (
            USER_BANNED_EVENT_KIND,
            normalize::moderation_banned(&payload),
        ),
        "App\\Events\\SubscriptionEvent" => (
            "kick.channel.subscribed",
            normalize::channel_subscribed(&payload),
        ),
        "App\\Events\\GiftedSubscriptionsEvent" => (
            "kick.channel.subscription.gifts",
            normalize::channel_subscription_gifted(&payload),
        ),
        "App\\Events\\StreamHostEvent" => {
            ("kick.channel.hosted", normalize::channel_hosted(&payload))
        }
        other => {
            debug!(event = %other, "unhandled Kick Pusher event");
            return None;
        }
    };

    Some(Event::new(EventSource::Kick, kind, normalized))
}

#[derive(Deserialize)]
struct PusherFrame {
    event: String,
    #[serde(default)]
    data: serde_json::Value,
}
