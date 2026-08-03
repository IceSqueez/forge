use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::Backoff;
use forge_platform_core::chat::ConnectionState;
use forge_types::ChatPayload;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::channel_info::ChannelInfoFetcher;
use crate::error::KickError;
use crate::normalize;

/// Community-observed (PLATFORMS_NOTES.md); re-verify via DevTools if events stop arriving.
pub const PUSHER_APP_KEY: &str = "32cbd69e4b950bf97679";

const PUSHER_WS_BASE: &str = "wss://ws-us2.pusher.com/app";
const PUSHER_PROTOCOL_PARAMS: &str = "protocol=7&client=js&version=7.6.0&flash=false";

const PING_INTERVAL: Duration = Duration::from_secs(30);

pub struct KickChat {
    slug: String,
    http: reqwest::Client,
}

pub struct KickChatHandle {
    close_tx: oneshot::Sender<()>,
    state_rx: watch::Receiver<ConnectionState>,
}

impl KickChatHandle {
    pub fn connection_state(&self) -> ConnectionState {
        *self.state_rx.borrow()
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.state_rx.clone()
    }

    pub fn shutdown(self) {
        let _ = self.close_tx.send(());
    }
}

impl KickChat {
    pub fn new(slug: String, http: reqwest::Client) -> Self {
        Self { slug, http }
    }

    fn pusher_ws_url() -> String {
        format!("{PUSHER_WS_BASE}/{PUSHER_APP_KEY}?{PUSHER_PROTOCOL_PARAMS}")
    }

    /// The returned handle's `shutdown` signals graceful shutdown; dropping it does the same.
    pub async fn connect(self, event_tx: mpsc::Sender<Event>) -> Result<KickChatHandle, KickError> {
        let fetcher = ChannelInfoFetcher::new(self.slug.clone(), self.http.clone());
        let channel_info = fetcher.fetch().await?;
        let chatroom_id = channel_info.chatroom_id;

        let ws_url = Self::pusher_ws_url();
        let ws_stream = connect_ws(&ws_url).await?;

        let (close_tx, close_rx) = oneshot::channel();
        let (state_tx, state_rx) = watch::channel(ConnectionState::Connecting);

        tokio::spawn(run_loop(
            ws_stream,
            chatroom_id,
            RunLoopContext {
                event_tx,
                close_rx,
                ws_url,
                slug: self.slug.clone(),
                http: self.http.clone(),
                state_tx,
            },
        ));

        Ok(KickChatHandle { close_tx, state_rx })
    }
}

async fn connect_ws(
    url: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    KickError,
> {
    let (ws_stream, _) =
        tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| KickError::WebSocket {
                reason: e.to_string(),
            })?;
    Ok(ws_stream)
}

async fn send_subscribe(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    chatroom_id: u64,
) -> Result<(), KickError> {
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

async fn send_ping(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<(), KickError> {
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
    close_rx: oneshot::Receiver<()>,
    ws_url: String,
    slug: String,
    http: reqwest::Client,
    state_tx: watch::Sender<ConnectionState>,
}

async fn run_loop(
    mut ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    chatroom_id: u64,
    ctx: RunLoopContext,
) {
    let RunLoopContext {
        event_tx,
        mut close_rx,
        ws_url,
        slug,
        http,
        state_tx,
    } = ctx;
    let mut backoff = Backoff::default();

    if let Err(e) = send_subscribe(&mut ws_stream, chatroom_id).await {
        warn!(error = %e, "subscribe send failed");
    }

    // subscription_succeeded is a Pusher internal frame we silently ignore, so WS-open +
    // subscribe-sent is treated as Connected; there is no better signal.
    let _ = state_tx.send(ConnectionState::Connected);

    let mut ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;

    'session: loop {
        tokio::select! {
            _ = &mut close_rx => {
                info!("kick chat close requested");
                let _ = state_tx.send(ConnectionState::Disconnected);
                return;
            }

            _ = tokio::time::sleep_until(ping_deadline) => {
                if let Err(e) = send_ping(&mut ws_stream).await {
                    warn!(error = %e, "ping failed; reconnecting");
                    break 'session;
                }
                ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;
            }

            msg = ws_stream.next() => {
                match msg {
                    None | Some(Err(_)) => {
                        warn!("kick chat WebSocket closed; reconnecting");
                        break 'session;
                    }
                    Some(Ok(Message::Text(text))) => {
                        handle_ws_text(&text, &event_tx).await;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        match frame {
                            Some(frame) => warn!(
                                code = %frame.code,
                                reason = %frame.reason,
                                "kick chat server sent close frame; reconnecting",
                            ),
                            None => warn!("kick chat server sent close frame; reconnecting"),
                        }
                        break 'session;
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    let _ = state_tx.send(ConnectionState::Reconnecting);
    tokio::time::sleep(backoff.next_delay()).await;

    loop {
        if matches!(
            close_rx.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        ) {
            let _ = state_tx.send(ConnectionState::Disconnected);
            return;
        }

        let fetcher = ChannelInfoFetcher::new(slug.clone(), http.clone());

        let new_chatroom_id = match fetcher.fetch().await {
            Ok(info) => info.chatroom_id,
            Err(e) => {
                warn!(error = %e, "channel info fetch failed on reconnect");
                tokio::time::sleep(backoff.next_delay()).await;
                continue;
            }
        };

        let Ok(new_ws) = connect_ws(&ws_url).await else {
            warn!("kick chat WS reconnect failed");
            tokio::time::sleep(backoff.next_delay()).await;
            continue;
        };

        ws_stream = new_ws;
        if let Err(e) = send_subscribe(&mut ws_stream, new_chatroom_id).await {
            warn!(error = %e, "subscribe failed on reconnect");
        }

        let _ = state_tx.send(ConnectionState::Connected);
        ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;

        let mut session_healthy = false;
        'session: loop {
            tokio::select! {
                _ = &mut close_rx => {
                    info!("kick chat close requested");
                    let _ = state_tx.send(ConnectionState::Disconnected);
                    return;
                }

                _ = tokio::time::sleep_until(ping_deadline) => {
                    if let Err(e) = send_ping(&mut ws_stream).await {
                        warn!(error = %e, "ping failed; reconnecting");
                        break 'session;
                    }
                    ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;
                }

                msg = ws_stream.next() => {
                    match msg {
                        None | Some(Err(_)) => {
                            warn!("kick chat WebSocket closed; reconnecting");
                            break 'session;
                        }
                        Some(Ok(Message::Text(text))) => {
                            if handle_ws_text(&text, &event_tx).await == WsFrameHealth::Healthy
                                && !session_healthy
                            {
                                session_healthy = true;
                                backoff.reset();
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            match frame {
                                Some(frame) => warn!(
                                    code = %frame.code,
                                    reason = %frame.reason,
                                    "kick chat server sent close frame; reconnecting",
                                ),
                                None => warn!("kick chat server sent close frame; reconnecting"),
                            }
                            break 'session;
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        let _ = state_tx.send(ConnectionState::Reconnecting);
        tokio::time::sleep(backoff.next_delay()).await;
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
        warn!(frame = %raw, "kick chat pusher error frame");
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
            ("kick.chat.message.sent", normalized)
        }
        "App\\Events\\MessageDeletedEvent" => (
            "kick.chat.message.deleted",
            normalize::chat_message_deleted(&payload),
        ),
        "App\\Events\\UserBannedEvent" => (
            "kick.moderation.banned",
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
mod tests {
    use super::*;
    use forge_events::EventSource;
    use tokio::sync::mpsc;

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

    /// Only a healthy frame resets the reconnect backoff, so a rejected subscription must not
    /// be mistaken for a working session.
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
}
