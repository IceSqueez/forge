use std::future::Future;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_net::Backoff;
use forge_platform_core::chat::ConnectionState;
use forge_platform_core::{ConnectionTransition, EndpointSurface, PlatformEndpoints};
use forge_types::ChatPayload;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ban_ledger::USER_BANNED_EVENT_KIND;
use crate::channel_info::ChannelInfoFetcher;
use crate::error::KickError;
use crate::normalize;

pub const PUSHER_APP_KEY: &str = "32cbd69e4b950bf97679";

const PUSHER_PROTOCOL_PARAMS: &str = "protocol=7&client=js&version=7.6.0&flash=false";

const PING_INTERVAL: Duration = Duration::from_secs(30);

const PUSHER_RECONNECT_IMMEDIATELY_CODES: std::ops::RangeInclusive<u16> = 4200..=4299;

const SESSION_STOP_GRACE: Duration = Duration::from_secs(5);

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
    fetcher: ChannelInfoFetcher,
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
    pub fn new(endpoints: &PlatformEndpoints, slug: String, http: reqwest::Client) -> Self {
        Self {
            fetcher: ChannelInfoFetcher::new(endpoints, slug, http),
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
        let Self { fetcher, ws_url } = self;
        let channel_info = cancel
            .run_until_cancelled(fetcher.fetch())
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
    let (ws_stream, _) =
        tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| KickError::WebSocket {
                reason: e.to_string(),
            })?;
    Ok(ws_stream)
}

async fn send_subscribe(ws: &mut PusherSocket, chatroom_id: u64) -> Result<(), KickError> {
    use futures_util::SinkExt;
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
    use futures_util::SinkExt;
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
    fetcher: ChannelInfoFetcher,
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
        let _ = ctx.state_tx.send(ChatLinkStatus {
            state: ConnectionState::Reconnecting,
            transition,
        });
        link = reconnect(&ctx, &mut backoff).await;
    }

    info!("kick chat close requested");
    let _ = ctx
        .state_tx
        .send(ChatLinkStatus::expected(ConnectionState::Disconnected));
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

    let _ = ctx
        .state_tx
        .send(ChatLinkStatus::unexpected(ConnectionState::Connected));
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

        let chatroom_id = match ctx.cancel.run_until_cancelled(ctx.fetcher.fetch()).await? {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use forge_events::EventSource;
    use tokio::sync::mpsc;

    pub(crate) fn handle_reporting(
        state: ConnectionState,
        cancel: CancellationToken,
    ) -> KickChatHandle {
        let (_state_tx, state_rx) = watch::channel(ChatLinkStatus::unexpected(state));
        KickChatHandle {
            cancel,
            tasks: JoinSet::new(),
            state_rx,
        }
    }

    struct RaiseOnDrop(Arc<AtomicBool>);

    impl Drop for RaiseOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum SessionTask {
        HonoursCancellation,
        IgnoresCancellation,
    }

    #[tokio::test(start_paused = true)]
    async fn stop_joins_a_cancelled_task_at_once_and_aborts_one_ignoring_cancellation_after_the_grace()
     {
        for (task, want_elapsed, want_finished) in [
            (SessionTask::HonoursCancellation, Duration::ZERO, true),
            (SessionTask::IgnoresCancellation, SESSION_STOP_GRACE, false),
        ] {
            let cancel = CancellationToken::new();
            let mut handle = handle_reporting(ConnectionState::Connected, cancel.clone());
            let finished = Arc::new(AtomicBool::new(false));
            let dropped = Arc::new(AtomicBool::new(false));
            let guard = RaiseOnDrop(Arc::clone(&dropped));
            let finished_flag = Arc::clone(&finished);
            handle.attach(async move {
                let _guard = guard;
                match task {
                    SessionTask::HonoursCancellation => cancel.cancelled().await,
                    SessionTask::IgnoresCancellation => std::future::pending::<()>().await,
                }
                finished_flag.store(true, Ordering::SeqCst);
            });
            tokio::task::yield_now().await;

            let started = tokio::time::Instant::now();
            handle.stop().await;

            assert_eq!(
                (
                    started.elapsed(),
                    finished.load(Ordering::SeqCst),
                    dropped.load(Ordering::SeqCst),
                ),
                (want_elapsed, want_finished, true),
                "{task:?}"
            );
        }
    }

    fn chat_payload() -> serde_json::Value {
        serde_json::json!({
            "id": "msg-uuid-1",
            "chatroom_id": 12345,
            "content": "hello world",
            "type": "message",
            "sender": {
                "id": 99,
                "username": "viewer_slug",
                "slug": "viewer_slug",
                "identity": { "color": "#FF0000", "badges": [] }
            }
        })
    }

    #[test]
    fn build_event_dispatches_per_pusher_event_type() {
        let cases = vec![
            (
                "App\\Events\\ChatMessageEvent",
                chat_payload(),
                Some("kick.chat.message.sent"),
            ),
            (
                "App\\Events\\MessageDeletedEvent",
                serde_json::json!({"message_id": "abc", "deleted_by": 5}),
                Some("kick.chat.message.deleted"),
            ),
            (
                "App\\Events\\UserBannedEvent",
                serde_json::json!({"user": {"id": 1, "username": "bad_user"}, "banned_by": {"id": 2}}),
                Some("kick.moderation.banned"),
            ),
            (
                "App\\Events\\SubscriptionEvent",
                serde_json::json!({"user_ids": [1], "username": "sub_user", "months": 1}),
                Some("kick.channel.subscribed"),
            ),
            (
                "App\\Events\\GiftedSubscriptionsEvent",
                serde_json::json!({"gifted_usernames": ["a", "b"], "gifter_username": "g"}),
                Some("kick.channel.subscription.gifts"),
            ),
            (
                "App\\Events\\StreamHostEvent",
                serde_json::json!({"host_username": "host_channel", "number_viewers": 150}),
                Some("kick.channel.hosted"),
            ),
            ("App\\Events\\Unknown", serde_json::Value::Null, None),
            ("pusher:pong", serde_json::Value::Null, None),
        ];
        for (event_type, payload, expected_kind) in cases {
            let result = build_event(event_type, payload);
            match expected_kind {
                Some(kind) => {
                    let ev = result.expect("must build event");
                    assert_eq!(ev.kind, kind, "kind for {event_type}");
                    assert_eq!(ev.source, EventSource::Kick);
                }
                None => assert!(result.is_none(), "must be None for {event_type}"),
            }
        }
    }

    #[tokio::test]
    async fn handle_ws_text_parses_chat_event_and_reports_a_healthy_frame() {
        let (tx, mut rx) = mpsc::channel(8);
        let inner = serde_json::to_string(&chat_payload()).unwrap();
        let frame = serde_json::json!({
            "event": "App\\Events\\ChatMessageEvent",
            "channel": "chatrooms.12345.v2",
            "data": inner
        });
        let health = handle_ws_text(&frame.to_string(), &tx).await;

        assert!(health == WsFrameHealth::Healthy);
        let event = rx.recv().await.unwrap();
        assert_eq!(event.kind, "kick.chat.message.sent");
        assert_eq!(event.source, EventSource::Kick);
    }

    #[tokio::test]
    async fn only_a_pusher_error_frame_is_reported_unhealthy() {
        let cases = [
            (
                r#"{"event":"pusher:error","data":"{\"code\":4004,\"message\":\"over quota\"}"}"#,
                WsFrameHealth::Error,
            ),
            (
                r#"{"event":"pusher:pong","data":{}}"#,
                WsFrameHealth::Healthy,
            ),
            (
                r#"{"event":"pusher_internal:subscription_succeeded","channel":"chatrooms.1.v2","data":"{}"}"#,
                WsFrameHealth::Healthy,
            ),
            (
                r#"{"event":"App\\Events\\ChatMessageEvent","data":"not json {"}"#,
                WsFrameHealth::Healthy,
            ),
            (
                r#"{"event":"App\\Events\\Unknown","data":"{}"}"#,
                WsFrameHealth::Healthy,
            ),
            ("this is not a Pusher frame", WsFrameHealth::Healthy),
        ];

        for (raw, expected_health) in cases {
            let (tx, mut rx) = mpsc::channel(8);
            let health = handle_ws_text(raw, &tx).await;
            assert!(health == expected_health, "wrong health for frame: {raw}");
            assert!(
                rx.try_recv().is_err(),
                "non-chat frame must not publish an event: {raw}"
            );
        }
    }

    #[test]
    fn build_event_publishes_normalized_payload_not_raw_wire_shape() {
        let event =
            build_event("App\\Events\\ChatMessageEvent", chat_payload()).expect("must build event");
        assert_eq!(
            event.payload["sender"]["display_name"],
            serde_json::json!("viewer_slug")
        );
        assert!(event.payload.get("slug").is_none());
        assert!(event.payload["sender"].get("identity").is_none());
    }

    #[test]
    fn chat_message_event_carries_the_unified_envelope_with_mapped_badges() {
        let mut raw = chat_payload();
        raw["sender"]["identity"]["badges"] = serde_json::json!([
            { "type": "moderator", "text": "Moderator" },
            { "type": "subscriber", "text": "Subscriber", "count": 4 },
        ]);
        let event = build_event("App\\Events\\ChatMessageEvent", raw).expect("must build event");

        let chat: ChatPayload = serde_json::from_value(event.payload[ChatPayload::KEY].clone())
            .expect("typed envelope");
        assert_eq!(
            chat.badges,
            vec![
                forge_types::UserBadge::Moderator,
                forge_types::UserBadge::Subscriber { months: 4 },
            ]
        );
    }

    #[test]
    fn chat_message_event_exposes_no_flat_badge_strings_beside_the_envelope() {
        let mut raw = chat_payload();
        raw["sender"]["identity"]["badges"] =
            serde_json::json!([{ "type": "moderator", "text": "Moderator" }]);
        let event = build_event("App\\Events\\ChatMessageEvent", raw).expect("must build event");

        assert!(event.payload.get("badges").is_none());
        assert!(event.payload["sender"].get("badges").is_none());
    }

    #[test]
    fn pusher_error_code_survives_every_observed_data_and_code_shape() {
        let cases = [
            (serde_json::json!({ "code": 4004 }), Some(4004)),
            (serde_json::json!({ "code": "4004" }), Some(4004)),
            (serde_json::json!(r#"{"code":4009}"#), Some(4009)),
            (serde_json::json!(r#"{"code":"4009"}"#), Some(4009)),
            (serde_json::json!({ "message": "over quota" }), None),
            (serde_json::json!({ "code": serde_json::Value::Null }), None),
            (serde_json::json!({ "code": { "value": 4004 } }), None),
            (serde_json::json!("not json at all"), None),
            (serde_json::Value::Null, None),
        ];

        for (data, expected) in cases {
            assert_eq!(pusher_error_code(&data), expected, "for data {data}");
        }
    }

    fn pusher_error_warnings(raw: &str) -> Vec<crate::log_capture::CapturedLine> {
        let (health, lines) = crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let (tx, _rx) = mpsc::channel(8);
            handle_ws_text(raw, &tx).await
        });
        assert!(
            health == WsFrameHealth::Error,
            "frame must report unhealthy"
        );
        crate::log_capture::forge_lines(&lines)
            .into_iter()
            .filter(|line| line.level == tracing::Level::WARN)
            .cloned()
            .collect()
    }

    #[test]
    fn pusher_error_warning_reports_only_the_code_and_the_frame_length() {
        let raw = r#"{"event":"pusher:error","data":"{\"code\":4009,\"message\":\"rejected\"}"}"#;
        let warns = pusher_error_warnings(raw);

        assert_eq!(warns.len(), 1, "expected one warn line, got {warns:?}");
        let line = &warns[0];
        assert_eq!(line.message(), "kick chat pusher error frame");
        assert_eq!(line.field("code"), Some("4009"));
        assert_eq!(
            line.field("frame_len"),
            Some(raw.len().to_string().as_str())
        );
        assert_eq!(line.field_names(), vec!["code", "frame_len", "message"]);
    }

    #[test]
    fn pusher_error_warning_never_echoes_the_frame_payload() {
        const SENTINEL: &str = "s3cret-chat-line";
        let frames = [
            format!(
                r#"{{"event":"pusher:error","data":"{{\"code\":4009,\"message\":\"{SENTINEL}\"}}"}}"#
            ),
            format!(r#"{{"event":"pusher:error","data":{{"message":"{SENTINEL}"}}}}"#),
        ];

        for raw in frames {
            let warns = pusher_error_warnings(&raw);
            assert_eq!(warns.len(), 1, "expected one warn line for {raw}");
            assert!(
                !warns[0].mentions(SENTINEL),
                "frame payload leaked into {:?}",
                warns[0].fields
            );
        }
    }

    #[test]
    fn channel_info_failure_drops_the_response_body_and_the_slug() {
        const SENTINEL: &str = "somechannel";
        let cases: [(KickError, Option<&str>); 5] = [
            (
                KickError::Http {
                    status: 403,
                    body: format!("<html>blocked {SENTINEL}</html>"),
                },
                Some("HTTP 403"),
            ),
            (
                KickError::ChannelInfoUnavailable {
                    slug: SENTINEL.to_owned(),
                    reason: "error sending request".to_owned(),
                },
                Some("error sending request"),
            ),
            (
                KickError::ChatroomIdNotFound {
                    slug: SENTINEL.to_owned(),
                },
                None,
            ),
            (
                KickError::Network {
                    reason: "connection reset".to_owned(),
                },
                Some("connection reset"),
            ),
            (
                KickError::WebSocket {
                    reason: "handshake failed".to_owned(),
                },
                Some("handshake failed"),
            ),
        ];

        for (error, must_keep) in cases {
            let rendered = channel_info_failure(&error);
            assert!(
                !rendered.contains(SENTINEL),
                "leaked sentinel in {rendered:?}"
            );
            if let Some(needle) = must_keep {
                assert!(
                    rendered.contains(needle),
                    "lost diagnostic {needle:?} in {rendered:?}"
                );
            }
        }
    }

    #[test]
    fn a_chat_message_frame_publishes_exactly_the_shared_fixture_payload() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/published_chat_message.json"
        ))
        .unwrap();

        let ev = build_event("App\\Events\\ChatMessageEvent", fixture["raw"].clone()).unwrap();

        let actual =
            serde_json::json!({ "source": ev.source, "kind": ev.kind, "payload": ev.payload });
        let expected = serde_json::json!({
            "source": fixture["source"],
            "kind": fixture["kind"],
            "payload": fixture["payload"],
        });
        assert_eq!(
            actual,
            expected,
            "published chat event drifted from the fixture; actual:\n{}",
            serde_json::to_string_pretty(&actual).unwrap()
        );
    }

    #[test]
    fn the_kick_channel_owner_resolves_to_the_broadcaster_rung() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/published_chat_message.json"
        ))
        .unwrap();
        let mut raw = fixture["raw"].clone();
        raw["sender"]["identity"]["badges"] = serde_json::json!([
            { "type": "broadcaster", "text": "Broadcaster" },
            { "type": "subscriber", "text": "Subscriber", "count": 3 }
        ]);

        let ev = build_event("App\\Events\\ChatMessageEvent", raw).unwrap();
        let chat: forge_types::ChatPayload =
            serde_json::from_value(ev.payload[forge_types::ChatPayload::KEY].clone()).unwrap();

        assert_eq!(
            forge_types::PermissionRung::from_badges(&chat.badges),
            forge_types::PermissionRung::Broadcaster
        );
    }

    #[test]
    fn a_chat_message_from_a_sender_without_a_numeric_id_carries_no_viewer_identity() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/published_chat_message.json"
        ))
        .unwrap();
        for id in [serde_json::Value::Null, serde_json::json!("99")] {
            let mut raw = fixture["raw"].clone();
            raw["sender"]["id"] = id.clone();
            let ev = build_event("App\\Events\\ChatMessageEvent", raw).unwrap();
            assert!(
                forge_types::ChatViewer::read(&ev.payload).is_none(),
                "sender id {id}"
            );
        }
    }
}
