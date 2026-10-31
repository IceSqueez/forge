use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_net::{Backoff, DEFAULT_BACKOFF_BASE, DEFAULT_BACKOFF_CAP};
use forge_platform_core::ConnectionState;
use forge_platform_core::{ConnectionTransition, EndpointSurface, PlatformEndpoints};
use forge_types::ChatPayload;
use futures::StreamExt;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::pusher::chatroom_lookup::ChannelInfoFetcher;
use crate::pusher::error::KickError;
use crate::pusher::normalize;
use crate::pusher::protocol::{self, InboundFrame, app_events};
use crate::vocabulary::event_kinds;

const PING_INTERVAL: Duration = Duration::from_secs(30);

const PUSHER_RECONNECT_IMMEDIATELY_CODES: std::ops::RangeInclusive<u16> = 4200..=4299;

const SESSION_STOP_GRACE: Duration = Duration::from_secs(5);

const PUSHER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

const SUBSCRIPTION_ACK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
pub struct PusherTiming {
    pub connect_timeout: Duration,
    pub subscription_ack_timeout: Duration,
    pub session_stop_grace: Duration,
    pub reconnect_backoff_base: Duration,
    pub reconnect_backoff_cap: Duration,
}

impl Default for PusherTiming {
    fn default() -> Self {
        Self {
            connect_timeout: PUSHER_CONNECT_TIMEOUT,
            subscription_ack_timeout: SUBSCRIPTION_ACK_TIMEOUT,
            session_stop_grace: SESSION_STOP_GRACE,
            reconnect_backoff_base: DEFAULT_BACKOFF_BASE,
            reconnect_backoff_cap: DEFAULT_BACKOFF_CAP,
        }
    }
}

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
    timing: PusherTiming,
}

pub struct KickChatHandle {
    cancel: CancellationToken,
    tasks: JoinSet<()>,
    state_rx: watch::Receiver<ChatLinkStatus>,
    stop_grace: Duration,
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
        if tokio::time::timeout(self.stop_grace, join_all(&mut self.tasks))
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
            ws_url: protocol::socket_url(endpoints.base_url(EndpointSurface::KickChatSocket)),
            timing: PusherTiming::default(),
        }
    }

    pub fn with_timing(mut self, timing: PusherTiming) -> Self {
        self.timing = timing;
        self
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
            timing,
        } = self;
        let channel_info = cancel
            .run_until_cancelled(fetcher.fetch(&slug))
            .await
            .ok_or(KickError::ConnectCancelled)??;
        let ws_stream = cancel
            .run_until_cancelled(connect_ws(&ws_url, timing.connect_timeout))
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
                timing,
            },
        ));

        Ok(KickChatHandle {
            cancel,
            tasks,
            state_rx,
            stop_grace: timing.session_stop_grace,
        })
    }
}

async fn connect_ws(url: &str, connect_timeout: Duration) -> Result<PusherSocket, KickError> {
    let (ws_stream, _) =
        tokio::time::timeout(connect_timeout, tokio_tungstenite::connect_async(url))
            .await
            .map_err(|_| KickError::WebSocket {
                reason: format!("connect timed out after {}s", connect_timeout.as_secs()),
            })?
            .map_err(|e| KickError::WebSocket {
                reason: e.to_string(),
            })?;
    Ok(ws_stream)
}

async fn send_frame(ws: &mut PusherSocket, frame: &serde_json::Value) -> Result<(), KickError> {
    use futures::SinkExt;
    ws.send(Message::Text(frame.to_string().into()))
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
    timing: PusherTiming,
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
    let mut backoff = Backoff::new(
        ctx.timing.reconnect_backoff_base,
        ctx.timing.reconnect_backoff_cap,
    );
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
        .run_until_cancelled(send_frame(
            ws_stream,
            &protocol::subscribe_frame(chatroom_id),
        ))
        .await
    {
        None => return SessionEnd::Closed,
        Some(Err(e)) => {
            warn!(error = %e, "subscribe send failed; reconnecting");
            return SessionEnd::Lost(ConnectionTransition::Unexpected);
        }
        Some(Ok(())) => {}
    }

    let mut ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;
    let ack_deadline = tokio::time::Instant::now() + ctx.timing.subscription_ack_timeout;
    let mut subscribed = false;
    let mut session_healthy = false;

    loop {
        tokio::select! {
            biased;

            () = ctx.cancel.cancelled() => return SessionEnd::Closed,

            () = tokio::time::sleep_until(ack_deadline), if !subscribed => {
                warn!(chatroom_id, "kick chat subscription was not acknowledged; reconnecting");
                return SessionEnd::Lost(ConnectionTransition::Unexpected);
            }

            () = tokio::time::sleep_until(ping_deadline) => {
                match ctx.cancel.run_until_cancelled(send_frame(ws_stream, &protocol::ping_frame())).await {
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
                    if health == WsFrameHealth::Subscribed && !subscribed {
                        subscribed = true;
                        ctx.state_tx
                            .send_replace(ChatLinkStatus::unexpected(ConnectionState::Connected));
                        info!(chatroom_id, "kick chat connected");
                    }
                    if health != WsFrameHealth::Error && !session_healthy {
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
            .run_until_cancelled(connect_ws(&ctx.ws_url, ctx.timing.connect_timeout))
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
    let frame: InboundFrame = match serde_json::from_str(raw) {
        Ok(f) => f,
        Err(e) => {
            debug!(error = %e, "unparseable Pusher frame; skipping");
            return WsFrameHealth::Healthy;
        }
    };

    let event_name = frame.event.as_str();

    if event_name == protocol::ERROR {
        match pusher_error_code(&frame.data) {
            Some(code) => warn!(code, frame_len = raw.len(), "kick chat pusher error frame"),
            None => warn!(frame_len = raw.len(), "kick chat pusher error frame"),
        }
        return WsFrameHealth::Error;
    }

    if event_name == protocol::SUBSCRIPTION_SUCCEEDED {
        return WsFrameHealth::Subscribed;
    }

    if event_name == protocol::PONG || event_name.starts_with(protocol::INTERNAL_EVENT_PREFIX) {
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
    let code = body.get(protocol::ERROR_CODE_KEY)?;
    code.as_i64().or_else(|| code.as_str()?.parse().ok())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WsFrameHealth {
    Healthy,
    Subscribed,
    Error,
}

pub(crate) fn build_event(event_name: &str, payload: serde_json::Value) -> Option<Event> {
    let (kind, normalized) = match event_name {
        app_events::CHAT_MESSAGE => {
            let mut normalized = normalize::chat_message_sent(&payload);
            let chat_payload = normalize::chat_message_chat_payload(&payload);
            normalized[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
            if let Some(viewer) = normalize::chat_message_viewer(&payload) {
                viewer.attach(&mut normalized);
            }
            (event_kinds::CHAT_MESSAGE_SENT, normalized)
        }
        app_events::MESSAGE_DELETED => (
            event_kinds::CHAT_MESSAGE_DELETED,
            normalize::chat_message_deleted(&payload),
        ),
        app_events::USER_BANNED => (
            event_kinds::MODERATION_BANNED,
            normalize::moderation_banned(&payload),
        ),
        app_events::SUBSCRIPTION => (
            event_kinds::CHANNEL_SUBSCRIBED,
            normalize::channel_subscribed(&payload),
        ),
        app_events::GIFTED_SUBSCRIPTIONS => (
            event_kinds::CHANNEL_SUBSCRIPTION_GIFTS,
            normalize::channel_subscription_gifted(&payload),
        ),
        app_events::STREAM_HOST => (
            event_kinds::CHANNEL_HOSTED,
            normalize::channel_hosted(&payload),
        ),
        other => {
            debug!(event = %other, "unhandled Kick Pusher event");
            return None;
        }
    };

    Some(Event::new(EventSource::Kick, kind, normalized))
}

#[cfg(test)]
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
            stop_grace: SESSION_STOP_GRACE,
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
    async fn only_a_pusher_error_frame_is_reported_unhealthy_and_only_the_ack_as_subscribed() {
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
                WsFrameHealth::Subscribed,
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

    fn pusher_error_warnings(raw: &str) -> Vec<crate::test_support::CapturedLine> {
        let (health, lines) = crate::test_support::capture_blocking(tracing::Level::TRACE, async {
            let (tx, _rx) = mpsc::channel(8);
            handle_ws_text(raw, &tx).await
        });
        assert!(
            health == WsFrameHealth::Error,
            "frame must report unhealthy"
        );
        crate::test_support::forge_lines(&lines)
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
            "../../tests/fixtures/published_chat_message.json"
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
            "../../tests/fixtures/published_chat_message.json"
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
            "../../tests/fixtures/published_chat_message.json"
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
