use std::time::Duration;

use forge_events::{Event, EventSource};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::channel_info::ChannelInfoFetcher;
use crate::error::KickError;
use crate::reconnect;

/// Community-observed Pusher app key used by kick.com as of 2024.
/// If Pusher events stop arriving, verify the current key via DevTools:
/// filter for requests to "ws-us2.pusher.com" in the Network tab.
pub const PUSHER_APP_KEY: &str = "eb1d5f283081a78b932c";

const PUSHER_WS_BASE: &str = "wss://ws-us2.pusher.com/app";
const PUSHER_PROTOCOL_PARAMS: &str = "protocol=7&client=js&version=7.6.0&flash=false";

const PING_INTERVAL: Duration = Duration::from_secs(30);

pub struct KickChat {
    slug: String,
    http: reqwest::Client,
}

pub struct KickChatHandle {
    pub close_tx: oneshot::Sender<()>,
}

impl KickChat {
    pub fn new(slug: String, http: reqwest::Client) -> Self {
        Self { slug, http }
    }

    fn pusher_ws_url() -> String {
        format!("{PUSHER_WS_BASE}/{PUSHER_APP_KEY}?{PUSHER_PROTOCOL_PARAMS}")
    }

    /// Bootstraps chatroom_id lookup, connects the Pusher WebSocket, and begins
    /// dispatching events. The returned handle's `close_tx` signals graceful shutdown.
    /// Reconnects automatically with exponential backoff capped at 60 s.
    pub async fn connect(self, event_tx: mpsc::Sender<Event>) -> Result<KickChatHandle, KickError> {
        let fetcher = ChannelInfoFetcher::new(self.slug.clone(), self.http.clone());
        let channel_info = fetcher.fetch().await?;
        let chatroom_id = channel_info.chatroom_id;

        let ws_url = Self::pusher_ws_url();
        let ws_stream = connect_ws(&ws_url).await?;

        let (close_tx, close_rx) = oneshot::channel();

        tokio::spawn(run_loop(
            ws_stream,
            chatroom_id,
            event_tx,
            close_rx,
            ws_url,
            self.slug.clone(),
            self.http.clone(),
        ));

        Ok(KickChatHandle { close_tx })
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

async fn run_loop(
    mut ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    chatroom_id: u64,
    event_tx: mpsc::Sender<Event>,
    mut close_rx: oneshot::Receiver<()>,
    ws_url: String,
    slug: String,
    http: reqwest::Client,
) {
    let mut attempt: u32 = 0;

    if let Err(e) = send_subscribe(&mut ws_stream, chatroom_id).await {
        warn!(error = %e, "subscribe send failed");
    }

    let mut ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;

    'session: loop {
        tokio::select! {
            _ = &mut close_rx => {
                info!("kick chat close requested");
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
                    Some(Ok(Message::Close(_))) => {
                        info!("kick chat server sent close frame; reconnecting");
                        break 'session;
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    attempt += 1;
    reconnect::wait(attempt.saturating_sub(1)).await;

    loop {
        if matches!(
            close_rx.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        ) {
            return;
        }

        let fetcher = ChannelInfoFetcher::new(slug.clone(), http.clone());

        let new_chatroom_id = match fetcher.fetch().await {
            Ok(info) => info.chatroom_id,
            Err(e) => {
                warn!(error = %e, "channel info fetch failed on reconnect");
                reconnect::wait(attempt).await;
                attempt += 1;
                continue;
            }
        };

        let Ok(new_ws) = connect_ws(&ws_url).await else {
            warn!("kick chat WS reconnect failed");
            reconnect::wait(attempt).await;
            attempt += 1;
            continue;
        };

        ws_stream = new_ws;
        if let Err(e) = send_subscribe(&mut ws_stream, new_chatroom_id).await {
            warn!(error = %e, "subscribe failed on reconnect");
        }

        ping_deadline = tokio::time::Instant::now() + PING_INTERVAL;
        attempt = 0;

        'session: loop {
            tokio::select! {
                _ = &mut close_rx => {
                    info!("kick chat close requested");
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
                        Some(Ok(Message::Close(_))) => {
                            info!("kick chat server sent close frame; reconnecting");
                            break 'session;
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        reconnect::wait(attempt).await;
        attempt += 1;
    }
}

async fn handle_ws_text(raw: &str, event_tx: &mpsc::Sender<Event>) {
    let frame: PusherFrame = match serde_json::from_str(raw) {
        Ok(f) => f,
        Err(e) => {
            debug!(error = %e, "unparseable Pusher frame; skipping");
            return;
        }
    };

    let event_name = frame.event.as_str();

    if event_name == "pusher:pong" || event_name.starts_with("pusher_internal:") {
        return;
    }

    let payload_str = match frame.data.as_str() {
        Some(s) => s.to_owned(),
        None => {
            debug!(event = %event_name, "Pusher frame data is not a string; skipping");
            return;
        }
    };

    let payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            debug!(error = %e, event = %event_name, "failed to parse event data; skipping");
            return;
        }
    };

    if let Some(event) = build_event(event_name, payload)
        && event_tx.send(event).await.is_err()
    {
        debug!("kick chat event receiver dropped");
    }
}

pub(crate) fn build_event(event_name: &str, payload: serde_json::Value) -> Option<Event> {
    let kind = match event_name {
        "App\\Events\\ChatMessageEvent" => "kick.chat",
        "App\\Events\\MessageDeletedEvent" => "kick.message_deleted",
        "App\\Events\\UserBannedEvent" => "kick.ban",
        "App\\Events\\SubscriptionEvent" => "kick.sub",
        "App\\Events\\GiftedSubscriptionsEvent" => "kick.sub_gift",
        "App\\Events\\StreamHostEvent" => "kick.host",
        other => {
            debug!(event = %other, "unhandled Kick Pusher event");
            return None;
        }
    };

    Some(Event::new(EventSource::Kick, kind, payload))
}

#[derive(Deserialize)]
struct PusherFrame {
    event: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    fn build_event_chat_message() {
        let event = build_event("App\\Events\\ChatMessageEvent", chat_payload()).unwrap();
        assert_eq!(event.kind, "kick.chat");
        assert_eq!(event.source, EventSource::Kick);
    }

    #[test]
    fn build_event_message_deleted() {
        let payload = serde_json::json!({"message_id": "abc", "deleted_by": 5});
        let event = build_event("App\\Events\\MessageDeletedEvent", payload).unwrap();
        assert_eq!(event.kind, "kick.message_deleted");
        assert_eq!(event.source, EventSource::Kick);
    }

    #[test]
    fn build_event_user_banned() {
        let payload =
            serde_json::json!({"user": {"id": 1, "username": "bad_user"}, "banned_by": {"id": 2}});
        let event = build_event("App\\Events\\UserBannedEvent", payload).unwrap();
        assert_eq!(event.kind, "kick.ban");
        assert_eq!(event.source, EventSource::Kick);
    }

    #[test]
    fn build_event_subscription() {
        let payload = serde_json::json!({"user_ids": [1], "username": "sub_user", "months": 1});
        let event = build_event("App\\Events\\SubscriptionEvent", payload).unwrap();
        assert_eq!(event.kind, "kick.sub");
    }

    #[test]
    fn build_event_gifted_subs() {
        let payload = serde_json::json!({"gifted_usernames": ["a", "b"], "gifter_username": "g"});
        let event = build_event("App\\Events\\GiftedSubscriptionsEvent", payload).unwrap();
        assert_eq!(event.kind, "kick.sub_gift");
    }

    #[test]
    fn build_event_host() {
        let payload = serde_json::json!({"host_username": "host_channel", "number_viewers": 150});
        let event = build_event("App\\Events\\StreamHostEvent", payload).unwrap();
        assert_eq!(event.kind, "kick.host");
    }

    #[test]
    fn build_event_unknown_returns_none() {
        let event = build_event("App\\Events\\Unknown", serde_json::Value::Null);
        assert!(event.is_none());
    }

    #[test]
    fn build_event_pusher_pong_skipped_in_handler() {
        assert!(build_event("pusher:pong", serde_json::Value::Null).is_none());
    }

    #[tokio::test]
    async fn handle_ws_text_parses_chat_event() {
        let (tx, mut rx) = mpsc::channel(8);
        let inner = serde_json::to_string(&chat_payload()).unwrap();
        let frame = serde_json::json!({
            "event": "App\\Events\\ChatMessageEvent",
            "channel": "chatrooms.12345.v2",
            "data": inner
        });
        handle_ws_text(&frame.to_string(), &tx).await;

        let event = rx.recv().await.unwrap();
        assert_eq!(event.kind, "kick.chat");
        assert_eq!(event.source, EventSource::Kick);
    }

    #[tokio::test]
    async fn handle_ws_text_ignores_pusher_pong() {
        let (tx, mut rx) = mpsc::channel(8);
        let frame = r#"{"event":"pusher:pong","data":{}}"#;
        handle_ws_text(frame, &tx).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_ws_text_ignores_subscription_succeeded() {
        let (tx, mut rx) = mpsc::channel(8);
        let frame = r#"{"event":"pusher_internal:subscription_succeeded","channel":"chatrooms.1.v2","data":"{}"}"#;
        handle_ws_text(frame, &tx).await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn handle_ws_text_ignores_unparseable_inner_data() {
        let (tx, mut rx) = mpsc::channel(8);
        let frame = serde_json::json!({
            "event": "App\\Events\\ChatMessageEvent",
            "data": "not json {"
        });
        handle_ws_text(&frame.to_string(), &tx).await;
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn pusher_app_key_is_nonempty() {
        assert!(!PUSHER_APP_KEY.is_empty());
    }
}
