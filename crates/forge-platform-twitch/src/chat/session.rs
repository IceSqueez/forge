use crate::chat::reconnect;
use crate::chat::subscriber::{SubscribeError, subscribe_all};
use crate::subscriptions::SubscriptionTracker;
use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_types::OAuthToken;
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};
use tokio::time::{Duration, Instant, sleep_until};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

const EVENTSUB_WS_URL: &str = "wss://eventsub.wss.twitch.tv/ws";
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatConnectionState {
    Connecting,
    Connected,
    Reconnecting { attempt: u8 },
    Disconnected,
}

struct SessionConfig {
    token: OAuthToken,
    client_id: String,
    broadcaster_id: String,
    user_id: String,
    bus: Arc<EventBus>,
    tracker: SubscriptionTracker,
}

pub(crate) struct ChatSession {
    config: SessionConfig,
    state_tx: watch::Sender<ChatConnectionState>,
    shutdown_rx: oneshot::Receiver<()>,
}

impl ChatSession {
    pub(crate) fn new(
        token: OAuthToken,
        client_id: String,
        broadcaster_id: String,
        user_id: String,
        bus: Arc<EventBus>,
        tracker: SubscriptionTracker,
    ) -> (
        Self,
        watch::Receiver<ChatConnectionState>,
        oneshot::Sender<()>,
    ) {
        let (state_tx, state_rx) = watch::channel(ChatConnectionState::Connecting);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let session = Self {
            config: SessionConfig {
                token,
                client_id,
                broadcaster_id,
                user_id,
                bus,
                tracker,
            },
            state_tx,
            shutdown_rx,
        };
        (session, state_rx, shutdown_tx)
    }

    pub(crate) async fn run(mut self) {
        let mut attempt: u32 = 0;
        let mut url = EVENTSUB_WS_URL.to_owned();

        loop {
            self.set_state(if attempt == 0 {
                ChatConnectionState::Connecting
            } else {
                ChatConnectionState::Reconnecting {
                    attempt: attempt.min(u8::MAX as u32) as u8,
                }
            });

            self.publish_connection_event();

            let outcome = self.run_session(&url).await;

            match outcome {
                SessionOutcome::Reconnect(new_url) => {
                    url = new_url;
                    attempt = 0;
                    continue;
                }
                SessionOutcome::Disconnected => {}
                SessionOutcome::ReauthRequired => {
                    warn!(
                        "twitch chat session stopped: token is missing required scope. \
                         Click Refresh token to re-authorize."
                    );
                    break;
                }
            }

            attempt += 1;
            reconnect::wait(attempt.saturating_sub(1)).await;

            if self.is_shutdown_requested() {
                break;
            }
        }

        self.set_state(ChatConnectionState::Disconnected);
        self.publish_connection_event();
    }

    async fn run_session(&mut self, url: &str) -> SessionOutcome {
        let ws_stream = match tokio_tungstenite::connect_async(url).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                warn!(error = %e, "WebSocket connect failed");
                return SessionOutcome::Disconnected;
            }
        };

        let mut ws_stream = ws_stream;
        let mut session_id: Option<String> = None;
        let mut keepalive_deadline = Instant::now() + KEEPALIVE_TIMEOUT;

        loop {
            tokio::select! {
                _ = sleep_until(keepalive_deadline) => {
                    warn!("keepalive timeout; treating as disconnect");
                    return SessionOutcome::Disconnected;
                }

                msg = ws_stream.next() => {
                    match msg {
                        None => return SessionOutcome::Disconnected,
                        Some(Err(e)) => {
                            warn!(error = %e, "WebSocket read error");
                            return SessionOutcome::Disconnected;
                        }
                        Some(Ok(Message::Text(text))) => {
                            keepalive_deadline = Instant::now() + KEEPALIVE_TIMEOUT;
                            match self.handle_frame(&text, &mut session_id).await {
                                FrameAction::Continue => {}
                                FrameAction::Reconnect(new_url) => {
                                    return SessionOutcome::Reconnect(new_url);
                                }
                                FrameAction::Disconnect => return SessionOutcome::Disconnected,
                                FrameAction::ReauthRequired => {
                                    return SessionOutcome::ReauthRequired;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("server sent close frame");
                            return SessionOutcome::Disconnected;
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }
    }

    async fn handle_frame(&mut self, text: &str, session_id: &mut Option<String>) -> FrameAction {
        let frame: WsFrame = match serde_json::from_str(text) {
            Ok(f) => f,
            Err(e) => {
                debug!(error = %e, "unrecognised WS frame; skipping");
                return FrameAction::Continue;
            }
        };

        match frame.metadata.message_type.as_str() {
            "session_welcome" => {
                let id = match frame.payload.as_ref().and_then(|p| p.session.as_ref()) {
                    Some(s) => s.id.clone(),
                    None => {
                        warn!("session_welcome missing session.id");
                        return FrameAction::Disconnect;
                    }
                };
                debug!("session_welcome received, subscribing topics");
                *session_id = Some(id.clone());

                match subscribe_all(
                    &self.config.token,
                    &self.config.client_id,
                    &id,
                    &self.config.broadcaster_id,
                    &self.config.user_id,
                    &self.config.bus,
                    &self.config.tracker,
                )
                .await
                {
                    Ok(_) => {
                        self.set_state(ChatConnectionState::Connected);
                        self.publish_connection_event();
                        info!(broadcaster_id = %self.config.broadcaster_id, "chat connected");
                    }
                    Err(SubscribeError::ScopeMissing) => {
                        warn!("chat subscription rejected: scope missing (reauth required)");
                        self.config.bus.publish(Event::new(
                            EventSource::Twitch,
                            "platform.reauth_required",
                            serde_json::json!({ "platform": "twitch" }),
                        ));
                        return FrameAction::ReauthRequired;
                    }
                }
            }

            "session_keepalive" => {
                debug!("keepalive received");
            }

            "session_reconnect" => {
                let new_url = frame
                    .payload
                    .as_ref()
                    .and_then(|p| p.session.as_ref())
                    .and_then(|s| s.reconnect_url.clone());
                match new_url {
                    Some(new_url) => {
                        info!(url = %new_url, "server-initiated reconnect");
                        return FrameAction::Reconnect(new_url);
                    }
                    None => {
                        warn!("session_reconnect missing reconnect_url");
                        return FrameAction::Disconnect;
                    }
                }
            }

            "notification" => {
                if let Some(payload) = &frame.payload
                    && let Some(event_data) = &payload.event
                {
                    debug!("chat notification received");
                    self.publish_chat_message(event_data);
                }
            }

            other => {
                debug!(message_type = %other, "unhandled WS message type");
            }
        }

        FrameAction::Continue
    }

    fn publish_chat_message(&self, event_data: &serde_json::Value) {
        let channel = event_data
            .get("broadcaster_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_login = event_data
            .get("chatter_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_id = event_data
            .get("chatter_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let message = event_data
            .get("message")
            .and_then(|m| m.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let color = event_data
            .get("color")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let roles = extract_roles_from_badges(event_data.get("badges"));
        let badges = roles.clone();

        info!(
            channel = %channel,
            user_login = %user_login,
            "chat message received"
        );

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "channel": channel,
                "user": {
                    "login": user_login,
                    "id": user_id,
                    "roles": roles,
                },
                "message": message,
                "badges": badges,
                "color": color,
            }),
        ));
    }

    fn set_state(&self, state: ChatConnectionState) {
        let _ = self.state_tx.send(state);
    }

    fn publish_connection_event(&self) {
        let state = *self.state_tx.borrow();
        let (state_str, attempt) = match state {
            ChatConnectionState::Connecting => ("connecting", None),
            ChatConnectionState::Connected => ("connected", None),
            ChatConnectionState::Reconnecting { attempt } => ("reconnecting", Some(attempt)),
            ChatConnectionState::Disconnected => ("disconnected", None),
        };
        let mut payload = serde_json::json!({
            "platform": "twitch",
            "state": state_str,
        });
        if let Some(n) = attempt {
            payload["attempt"] = serde_json::json!(n);
        }
        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "platform.connection.changed",
            payload,
        ));
    }

    fn is_shutdown_requested(&mut self) -> bool {
        matches!(
            self.shutdown_rx.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        )
    }
}

fn extract_roles_from_badges(badges: Option<&serde_json::Value>) -> Vec<String> {
    badges
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| b.get("set_id").and_then(|s| s.as_str()))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

enum SessionOutcome {
    Reconnect(String),
    Disconnected,
    ReauthRequired,
}

enum FrameAction {
    Continue,
    Reconnect(String),
    Disconnect,
    ReauthRequired,
}

#[derive(Debug, Deserialize)]
struct WsFrame {
    metadata: FrameMetadata,
    payload: Option<FramePayload>,
}

#[derive(Debug, Deserialize)]
struct FrameMetadata {
    message_type: String,
}

#[derive(Debug, Deserialize)]
struct FramePayload {
    session: Option<SessionInfo>,
    event: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SessionInfo {
    id: String,
    reconnect_url: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn ws_frame_deserializes_session_welcome() {
        let raw = r#"{"metadata":{"message_type":"session_welcome","message_id":"abc"},"payload":{"session":{"id":"sess-123","reconnect_url":null}}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        assert_eq!(frame.metadata.message_type, "session_welcome");
        let sid = frame.payload.unwrap().session.unwrap().id;
        assert_eq!(sid, "sess-123");
    }

    #[test]
    fn ws_frame_deserializes_session_reconnect() {
        let raw = r#"{"metadata":{"message_type":"session_reconnect"},"payload":{"session":{"id":"sess-456","reconnect_url":"wss://eventsub.wss.twitch.tv/ws?reconnect_token=abc"}}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        assert_eq!(frame.metadata.message_type, "session_reconnect");
        let url = frame
            .payload
            .unwrap()
            .session
            .unwrap()
            .reconnect_url
            .unwrap();
        assert!(url.contains("reconnect_token=abc"));
    }

    #[test]
    fn ws_frame_deserializes_notification_with_event() {
        let raw = r#"{"metadata":{"message_type":"notification"},"payload":{"event":{"broadcaster_user_id":"12345","chatter_user_id":"67890","chatter_user_login":"someuser","message_id":"msg-001","message":{"text":"Hello!"},"color":"red"}}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        let event = frame.payload.unwrap().event.unwrap();
        assert_eq!(event["chatter_user_login"], "someuser");
        assert_eq!(event["message"]["text"], "Hello!");
    }

    #[test]
    fn ws_frame_deserializes_keepalive() {
        let raw = r#"{"metadata":{"message_type":"session_keepalive"},"payload":{}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        assert_eq!(frame.metadata.message_type, "session_keepalive");
    }

    #[test]
    fn chat_connection_state_equality() {
        assert_eq!(
            ChatConnectionState::Connected,
            ChatConnectionState::Connected
        );
        assert_ne!(
            ChatConnectionState::Reconnecting { attempt: 1 },
            ChatConnectionState::Reconnecting { attempt: 2 }
        );
    }

    #[test]
    fn extract_roles_from_badges_returns_set_ids() {
        let badges = serde_json::json!([
            {"set_id": "moderator", "id": "1", "info": ""},
            {"set_id": "subscriber", "id": "3012", "info": "36"}
        ]);
        let roles = extract_roles_from_badges(Some(&badges));
        assert_eq!(roles, vec!["moderator", "subscriber"]);
    }

    #[test]
    fn extract_roles_from_badges_empty_array() {
        let badges = serde_json::json!([]);
        let roles = extract_roles_from_badges(Some(&badges));
        assert!(roles.is_empty());
    }

    #[test]
    fn extract_roles_from_badges_missing_field() {
        let roles = extract_roles_from_badges(None);
        assert!(roles.is_empty());
    }

    #[tokio::test]
    async fn publish_chat_message_emits_rfc031_payload_shape() {
        use forge_runtime::{EventBus, NullEventLogRepo};
        use forge_types::OAuthToken;
        use std::sync::Arc;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let token = OAuthToken::new("dummy".to_string());
        let tracker = crate::subscriptions::SubscriptionTracker::default();
        let (session, _, _) = ChatSession::new(
            token,
            "client".to_string(),
            "bcast".to_string(),
            "user".to_string(),
            Arc::clone(&bus),
            tracker,
        );
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "broadcaster_user_login": "streamer_channel",
            "chatter_user_id": "67890",
            "chatter_user_login": "viewer_one",
            "message": {"text": "hello world"},
            "color": "#FF0000",
            "badges": [
                {"set_id": "moderator", "id": "1", "info": ""},
                {"set_id": "vip", "id": "1", "info": ""}
            ]
        });

        session.publish_chat_message(&event_data);

        let ev = tokio::time::timeout(std::time::Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        assert_eq!(ev.kind, "chat.message");
        assert_eq!(ev.source, EventSource::Twitch);
        assert_eq!(ev.payload["channel"].as_str(), Some("streamer_channel"));
        assert_eq!(ev.payload["user"]["login"].as_str(), Some("viewer_one"));
        assert_eq!(ev.payload["user"]["id"].as_str(), Some("67890"));
        assert_eq!(ev.payload["message"].as_str(), Some("hello world"));
        assert_eq!(ev.payload["color"].as_str(), Some("#FF0000"));
        let roles = ev.payload["user"]["roles"].as_array().unwrap();
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[0].as_str(), Some("moderator"));
        assert_eq!(roles[1].as_str(), Some("vip"));
        let badges = ev.payload["badges"].as_array().unwrap();
        assert_eq!(badges[0].as_str(), Some("moderator"));
    }
}
