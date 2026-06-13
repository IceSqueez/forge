use crate::chat::reconnect;
use crate::chat::subscriber::{SubscribeError, subscribe_all};
use crate::subscriptions::SubscriptionTracker;
use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_types::{ChatPayload, OAuthToken};
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};
use tokio::time::{Duration, Instant, sleep_until};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use super::dispatch;
use super::payload;

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
                let sub_type = frame.metadata.subscription_type.as_deref().unwrap_or("");
                let frame_msg_id = frame.metadata.message_id.as_str();
                if let Some(frame_payload) = &frame.payload
                    && let Some(event_data) = &frame_payload.event
                {
                    match dispatch::route_for(sub_type) {
                        Some(route) => route(self, event_data, frame_msg_id),
                        None => {
                            debug!(subscription_type = %sub_type, "no route registered for notification subscription type");
                        }
                    }
                }
            }

            other => {
                debug!(message_type = %other, "unhandled WS message type");
            }
        }

        FrameAction::Continue
    }

    pub(super) fn publish_chat_message(&self, event_data: &serde_json::Value) {
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

        let chat_payload = payload::build_chat_message_chat_payload(event_data);
        let mut forge_payload = serde_json::json!({
            "channel": channel,
            "user": {
                "login": user_login,
                "id": user_id,
                "roles": roles,
            },
            "message": message,
            "badges": badges,
            "color": color,
        });

        if let Some(bits) = event_data
            .get("cheer")
            .and_then(|c| c.get("bits"))
            .and_then(|v| v.as_i64())
        {
            // channel.chat.message cheer object carries only {bits}; no anonymity signal.
            forge_payload["cheer"] = serde_json::json!({ "bits": bits });
        }

        let broadcaster_id = event_data
            .get("broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let source_id = event_data
            .get("source_broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        // In a shared-chat session Twitch echoes source_broadcaster_* on every message including
        // the host's own. Surface from_channel only when the message originates from a different
        // channel (source present and not the host).
        if !source_id.is_empty()
            && source_id != broadcaster_id
            && let (Some(login), Some(display_name)) = (
                event_data
                    .get("source_broadcaster_user_login")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty()),
                event_data
                    .get("source_broadcaster_user_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty()),
            )
        {
            forge_payload["from_channel"] = serde_json::json!({
                "login": login,
                "display_name": display_name,
            });
        }

        attach_chat_payload(&mut forge_payload, chat_payload);

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            forge_payload,
        ));
    }

    pub(super) fn publish_subscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user_login = event_data
            .get("user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_id = event_data
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_display = event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let tier = event_data
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let is_gift = event_data
            .get("is_gift")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        info!(user_login = %user_login, "subscriber event received");

        let chat_payload = payload::build_subscribe_chat_payload(event_data, frame_msg_id);
        let mut forge_payload = serde_json::json!({
            "user": { "id": user_id, "login": user_login, "display_name": user_display },
            "tier": tier,
            "is_gift": is_gift,
        });
        attach_chat_payload(&mut forge_payload, chat_payload);

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.subscribe",
            forge_payload,
        ));
    }

    pub(super) fn publish_resubscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user_login = event_data
            .get("user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_id = event_data
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_display = event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let tier = event_data
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let cumulative_months = event_data
            .get("cumulative_months")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let streak_months = event_data
            .get("streak_months")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let message_text = event_data
            .get("message")
            .and_then(|m| m.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let share_streak = event_data
            .get("share_streak")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        info!(user_login = %user_login, cumulative_months = cumulative_months, "resub event received");

        let chat_payload = payload::build_resubscribe_chat_payload(event_data, frame_msg_id);
        let mut forge_payload = serde_json::json!({
            "user": { "id": user_id, "login": user_login, "display_name": user_display },
            "tier": tier,
            "cumulative_months": cumulative_months,
            "streak_months": streak_months,
            "message": message_text,
            "share_streak": share_streak,
        });
        attach_chat_payload(&mut forge_payload, chat_payload);

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.subscription.message",
            forge_payload,
        ));
    }

    pub(super) fn publish_gift_sub_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let gifter_login = event_data
            .get("user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let gifter_id = event_data
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let gifter_display = event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let is_anonymous = event_data
            .get("is_anonymous")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let tier = event_data
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        info!(gifter_login = %gifter_login, "gift sub event received");

        let chat_payload = payload::build_gift_sub_chat_payload(event_data, frame_msg_id);
        let mut forge_payload = serde_json::json!({
            "tier": tier,
            "is_anonymous": is_anonymous,
            "gifter": { "id": gifter_id, "login": gifter_login, "display_name": gifter_display },
            "recipient": { "id": "", "login": "", "display_name": "" },
        });
        attach_chat_payload(&mut forge_payload, chat_payload);

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.subscription.gift",
            forge_payload,
        ));
    }

    pub(super) fn publish_cheer_event(&self, event_data: &serde_json::Value, frame_msg_id: &str) {
        let user_login = event_data
            .get("user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_id = event_data
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_display = event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let bits = event_data.get("bits").and_then(|v| v.as_i64()).unwrap_or(0);
        let message = event_data
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let is_anonymous = event_data
            .get("is_anonymous")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        info!(user_login = %user_login, bits = bits, "cheer event received");

        let chat_payload = payload::build_cheer_chat_payload(event_data, frame_msg_id);
        let mut forge_payload = serde_json::json!({
            "bits": bits,
            "message": message,
            "is_anonymous": is_anonymous,
            "user": { "id": user_id, "login": user_login, "display_name": user_display },
        });
        attach_chat_payload(&mut forge_payload, chat_payload);

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.cheer",
            forge_payload,
        ));
    }

    pub(super) fn publish_raid_event(&self, event_data: &serde_json::Value, frame_msg_id: &str) {
        let from_login = event_data
            .get("from_broadcaster_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let from_id = event_data
            .get("from_broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let from_display = event_data
            .get("from_broadcaster_user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let viewer_count = event_data
            .get("viewers")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        info!(from_login = %from_login, viewer_count = viewer_count, "raid event received");

        let chat_payload = payload::build_raid_chat_payload(event_data, frame_msg_id);
        let mut forge_payload = serde_json::json!({
            "viewer_count": viewer_count,
            "from_broadcaster": {
                "id": from_id,
                "login": from_login,
                "display_name": from_display,
            },
        });
        attach_chat_payload(&mut forge_payload, chat_payload);

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.raid",
            forge_payload,
        ));
    }

    pub(super) fn publish_reward_redemption_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let redemption_id = event_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let redemption_status = event_data
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_input = event_data
            .get("user_input")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let redeemed_at = event_data
            .get("redeemed_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_id = event_data
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_login = event_data
            .get("user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_name = event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let reward_id = event_data
            .get("reward")
            .and_then(|r| r.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let reward_title = event_data
            .get("reward")
            .and_then(|r| r.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let reward_cost = event_data
            .get("reward")
            .and_then(|r| r.get("cost"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let reward_prompt = event_data
            .get("reward")
            .and_then(|r| r.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        info!(user_login = %user_login, reward_title = %reward_title, "channel point reward redemption received");

        let forge_payload = serde_json::json!({
            "redemption": {
                "id": redemption_id,
                "status": redemption_status,
                "user_input": user_input,
                "redeemed_at": redeemed_at,
            },
            "user": {
                "id": user_id,
                "login": user_login,
                "display_name": user_name,
            },
            "reward": {
                "id": reward_id,
                "title": reward_title,
                "cost": reward_cost,
                "prompt": reward_prompt,
            },
        });

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.channel_points_redemption",
            forge_payload,
        ));
    }

    pub(super) fn publish_message_delete_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let message_id = event_data
            .get("message_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let target_user_id = event_data
            .get("target_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let target_user_login = event_data
            .get("target_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let target_user_name = event_data
            .get("target_user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        // The channel.chat.message_delete event carries no deleted text and no
        // moderator identity — Twitch does not include those fields in this topic.
        info!(target_user_login = %target_user_login, message_id = %message_id, "chat message deleted");

        let forge_payload = serde_json::json!({
            "message_id": message_id,
            "target_user": {
                "id": target_user_id,
                "login": target_user_login,
                "display_name": target_user_name,
            },
        });

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.chat.message_delete",
            forge_payload,
        ));
    }

    pub(super) fn publish_chat_clear_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster_id = event_data
            .get("broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let broadcaster_login = event_data
            .get("broadcaster_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        // The channel.chat.clear event carries no moderator identity — Twitch
        // does not include that field in this topic.
        info!(broadcaster_login = %broadcaster_login, "chat cleared");

        let forge_payload = serde_json::json!({
            "broadcaster": {
                "id": broadcaster_id,
                "login": broadcaster_login,
            },
        });

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.chat.clear",
            forge_payload,
        ));
    }

    pub(super) fn publish_follow_event(&self, event_data: &serde_json::Value, _frame_msg_id: &str) {
        let user_login = event_data
            .get("user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_id = event_data
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let user_name = event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let followed_at = event_data
            .get("followed_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        info!(user_login = %user_login, "follow event received");

        let forge_payload = serde_json::json!({
            "followed_at": followed_at,
            "user": { "id": user_id, "login": user_login, "display_name": user_name },
        });

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "channel.follow",
            forge_payload,
        ));
    }

    pub(super) fn publish_stream_online_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let stream_id = event_data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let stream_type = event_data
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let started_at = event_data
            .get("started_at")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let broadcaster_login = event_data
            .get("broadcaster_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let broadcaster_id = event_data
            .get("broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        info!(broadcaster_login = %broadcaster_login, "stream online event received");

        let forge_payload = serde_json::json!({
            "stream": { "id": stream_id, "type": stream_type, "started_at": started_at },
            "broadcaster": { "id": broadcaster_id, "login": broadcaster_login },
        });

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "stream.online",
            forge_payload,
        ));
    }

    pub(super) fn publish_stream_offline_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster_login = event_data
            .get("broadcaster_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let broadcaster_id = event_data
            .get("broadcaster_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        info!(broadcaster_login = %broadcaster_login, "stream offline event received");

        let forge_payload = serde_json::json!({
            "broadcaster": { "id": broadcaster_id, "login": broadcaster_login },
        });

        self.config.bus.publish(Event::new(
            EventSource::Twitch,
            "stream.offline",
            forge_payload,
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

fn attach_chat_payload(forge_payload: &mut serde_json::Value, chat: ChatPayload) {
    match serde_json::to_value(&chat) {
        Ok(chat_value) => {
            if let serde_json::Value::Object(map) = forge_payload {
                map.insert(ChatPayload::KEY.to_owned(), chat_value);
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to serialize ChatPayload; _chat key omitted");
        }
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
    #[serde(default)]
    message_id: String,
    message_type: String,
    subscription_type: Option<String>,
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
    use std::time::Duration;

    use forge_events::EventSource;
    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_types::{ChatEventDetail, ChatPayload, ChatSegment, OAuthToken};

    use super::*;

    fn make_session(bus: &Arc<EventBus>) -> ChatSession {
        let token = OAuthToken::new("dummy".to_string());
        let tracker = crate::subscriptions::SubscriptionTracker::default();
        let (session, _, _) = ChatSession::new(
            token,
            "client".to_string(),
            "bcast".to_string(),
            "user".to_string(),
            Arc::clone(bus),
            tracker,
        );
        session
    }

    #[test]
    fn ws_frame_deserializes_session_welcome() {
        let raw = r#"{"metadata":{"message_type":"session_welcome","message_id":"abc"},"payload":{"session":{"id":"sess-123","reconnect_url":null}}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        assert_eq!(frame.metadata.message_type, "session_welcome");
        assert_eq!(frame.metadata.message_id, "abc");
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
    fn ws_frame_deserializes_notification_with_subscription_type() {
        let raw = r#"{"metadata":{"message_type":"notification","message_id":"notif-001","subscription_type":"channel.chat.message"},"payload":{"event":{"chatter_user_login":"viewer","message":{"text":"hi"}}}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        assert_eq!(frame.metadata.message_type, "notification");
        assert_eq!(
            frame.metadata.subscription_type.as_deref(),
            Some("channel.chat.message")
        );
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
    fn ws_frame_message_id_defaults_to_empty_when_absent() {
        let raw = r#"{"metadata":{"message_type":"session_keepalive"},"payload":{}}"#;
        let frame: WsFrame = serde_json::from_str(raw).expect("must parse");
        assert_eq!(frame.metadata.message_id, "");
    }

    #[test]
    fn extract_roles_from_badges_returns_set_ids_skipping_malformed_entries() {
        let badges = serde_json::json!([
            {"set_id": "moderator", "id": "1", "info": ""},
            {"set_id": "subscriber", "id": "3012", "info": "36"},
            {"id": "no-set-id"}
        ]);
        let roles = extract_roles_from_badges(Some(&badges));
        assert_eq!(roles, vec!["moderator", "subscriber"]);
    }

    #[test]
    fn extract_roles_from_badges_yields_empty_for_missing_or_non_array_badges() {
        for badges in [
            None,
            Some(serde_json::json!([])),
            Some(serde_json::json!("not-an-array")),
        ] {
            assert!(
                extract_roles_from_badges(badges.as_ref()).is_empty(),
                "expected empty roles for {badges:?}"
            );
        }
    }

    #[tokio::test]
    async fn publish_chat_message_emits_rfc031_payload_shape() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
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

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
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

    #[tokio::test]
    async fn publish_chat_message_surfaces_cheer_bits_without_anonymity_signal() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "broadcaster_user_login": "streamer",
            "chatter_user_id": "67890",
            "chatter_user_login": "viewer",
            "message": {"text": "cheer100 gg"},
            "cheer": {"bits": 100},
            "badges": []
        });
        session.publish_chat_message(&event_data);

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        assert_eq!(ev.payload["cheer"]["bits"].as_i64(), Some(100));
        assert!(
            ev.payload["cheer"].get("is_anonymous").is_none(),
            "channel.chat.message cheer carries no anonymity signal"
        );
    }

    #[tokio::test]
    async fn publish_chat_message_surfaces_from_channel_for_shared_chat_source() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "broadcaster_user_login": "host",
            "broadcaster_user_id": "100",
            "chatter_user_id": "42",
            "chatter_user_login": "guest_viewer",
            "message": {"text": "hello from elsewhere"},
            "source_broadcaster_user_id": "200",
            "source_broadcaster_user_login": "other_chan",
            "source_broadcaster_user_name": "OtherChan",
            "badges": []
        });
        session.publish_chat_message(&event_data);

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        assert_eq!(
            ev.payload["from_channel"]["login"].as_str(),
            Some("other_chan")
        );
        assert_eq!(
            ev.payload["from_channel"]["display_name"].as_str(),
            Some("OtherChan")
        );
    }

    #[tokio::test]
    async fn publish_chat_message_omits_from_channel_for_own_channel_shared_chat_echo() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        // Shared-chat session echoes source_broadcaster_* on the host's own messages;
        // when source id == broadcaster id, from_channel must NOT surface.
        let event_data = serde_json::json!({
            "broadcaster_user_login": "host",
            "broadcaster_user_id": "100",
            "chatter_user_id": "42",
            "chatter_user_login": "host_viewer",
            "message": {"text": "my own channel"},
            "source_broadcaster_user_id": "100",
            "source_broadcaster_user_login": "host",
            "source_broadcaster_user_name": "Host",
            "badges": []
        });
        session.publish_chat_message(&event_data);

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        assert!(
            ev.payload.get("from_channel").is_none(),
            "own-channel echo must not surface from_channel"
        );
    }

    #[tokio::test]
    async fn publish_chat_message_omits_cheer_and_from_channel_when_not_applicable() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let plain = serde_json::json!({
            "broadcaster_user_login": "streamer",
            "chatter_user_id": "1",
            "chatter_user_login": "viewer",
            "message": {"text": "plain"},
            "badges": []
        });
        let empty_source = serde_json::json!({
            "broadcaster_user_login": "streamer",
            "broadcaster_user_id": "100",
            "chatter_user_id": "1",
            "chatter_user_login": "viewer",
            "message": {"text": "plain"},
            "source_broadcaster_user_id": "",
            "source_broadcaster_user_login": "",
            "source_broadcaster_user_name": "",
            "badges": []
        });

        for (name, event_data) in [("keys absent", plain), ("empty source id", empty_source)] {
            session.publish_chat_message(&event_data);

            let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
                .await
                .expect("timeout")
                .expect("recv error");

            assert!(ev.payload.get("cheer").is_none(), "cheer present: {name}");
            assert!(
                ev.payload.get("from_channel").is_none(),
                "from_channel present: {name}"
            );
        }
    }

    #[tokio::test]
    async fn chat_message_attaches_chat_payload() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "broadcaster_user_login": "streamer",
            "chatter_user_id": "123",
            "chatter_user_login": "viewer",
            "chatter_user_name": "Viewer",
            "message_id": "msg-abc",
            "message": {
                "text": "hello KEKW",
                "fragments": [
                    { "type": "text", "text": "hello " },
                    { "type": "emote", "text": "KEKW", "emote": { "id": "55edbc60" } }
                ]
            },
            "color": "#FF0000",
            "badges": []
        });
        session.publish_chat_message(&event_data);

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        let chat_val = ev.payload.get(ChatPayload::KEY).expect("_chat key missing");
        let chat: ChatPayload = serde_json::from_value(chat_val.clone()).unwrap();
        assert_eq!(chat.platform_msg_id, "msg-abc");
        assert_eq!(chat.author, "Viewer");
        assert!(!chat.is_event);
        assert_eq!(chat.segments.len(), 2);
        assert!(matches!(chat.segments[0], ChatSegment::Text { .. }));
        assert!(matches!(chat.segments[1], ChatSegment::Emote { .. }));
    }

    #[tokio::test]
    async fn chat_payload_is_additive_to_existing_argstack_keys() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "broadcaster_user_login": "chan",
            "chatter_user_id": "456",
            "chatter_user_login": "bob",
            "chatter_user_name": "Bob",
            "message_id": "m1",
            "message": {
                "text": "hi",
                "fragments": [{ "type": "text", "text": "hi" }]
            },
            "color": "#AABBCC",
            "badges": []
        });
        session.publish_chat_message(&event_data);

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(ev.payload.get("channel").is_some(), "channel key missing");
        assert!(ev.payload.get("user").is_some(), "user key missing");
        assert!(ev.payload.get("message").is_some(), "message key missing");
        assert!(ev.payload.get("color").is_some(), "color key missing");
        assert!(ev.payload.get("badges").is_some(), "badges key missing");
        let chat_val = ev.payload.get(ChatPayload::KEY);
        assert!(chat_val.is_some(), "_chat key must be present");
        let _chat: ChatPayload = serde_json::from_value(chat_val.unwrap().clone()).unwrap();
    }

    #[tokio::test]
    async fn subscriber_event_attaches_chat_payload_with_event_detail() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "user_id": "111",
            "user_login": "newbie",
            "user_name": "Newbie",
            "tier": "1000",
            "is_gift": false
        });
        session.publish_subscribe_event(&event_data, "meta-msg-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "channel.subscribe");
        let chat_val = ev.payload.get(ChatPayload::KEY).expect("_chat key missing");
        let chat: ChatPayload = serde_json::from_value(chat_val.clone()).unwrap();
        assert!(chat.is_event);
        assert!(chat.event_detail.is_some());
        assert!(
            matches!(
                chat.event_detail.unwrap(),
                ChatEventDetail::Subscription {
                    tier: 1,
                    months: None,
                    message: None
                }
            ),
            "expected Subscription {{ tier: 1, months: None, message: None }}"
        );
    }

    #[tokio::test]
    async fn raid_received_attaches_event_detail_raid() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "from_broadcaster_user_id": "666",
            "from_broadcaster_user_login": "big_streamer",
            "from_broadcaster_user_name": "BigStreamer",
            "to_broadcaster_user_id": "777",
            "viewers": 500u64
        });
        session.publish_raid_event(&event_data, "meta-raid-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "channel.raid");
        assert_eq!(ev.payload["viewer_count"].as_i64(), Some(500));
        assert_eq!(
            ev.payload["from_broadcaster"]["login"].as_str(),
            Some("big_streamer")
        );
        let chat_val = ev.payload.get(ChatPayload::KEY).expect("_chat key missing");
        let chat: ChatPayload = serde_json::from_value(chat_val.clone()).unwrap();
        assert!(chat.is_event);
        assert!(
            matches!(
                chat.event_detail,
                Some(ChatEventDetail::Raid { viewer_count: 500 })
            ),
            "expected Raid {{ viewer_count: 500 }}"
        );
    }

    #[tokio::test]
    async fn follow_event_publishes_nested_user_and_followed_at() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "user_id": "42",
            "user_login": "new_follower",
            "user_name": "NewFollower",
            "followed_at": "2026-06-13T10:00:00Z"
        });
        session.publish_follow_event(&event_data, "meta-follow-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "channel.follow");
        assert_eq!(ev.source, EventSource::Twitch);
        assert_eq!(ev.payload["user"]["login"].as_str(), Some("new_follower"));
        assert_eq!(ev.payload["user"]["id"].as_str(), Some("42"));
        assert_eq!(
            ev.payload["user"]["display_name"].as_str(),
            Some("NewFollower")
        );
        assert_eq!(
            ev.payload["followed_at"].as_str(),
            Some("2026-06-13T10:00:00Z")
        );
    }

    #[tokio::test]
    async fn stream_online_event_publishes_nested_stream_and_broadcaster() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "id": "stream-1",
            "type": "live",
            "started_at": "2026-06-13T09:00:00Z",
            "broadcaster_user_id": "100",
            "broadcaster_user_login": "host_chan"
        });
        session.publish_stream_online_event(&event_data, "meta-online-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "stream.online");
        assert_eq!(ev.payload["stream"]["id"].as_str(), Some("stream-1"));
        assert_eq!(ev.payload["stream"]["type"].as_str(), Some("live"));
        assert_eq!(
            ev.payload["stream"]["started_at"].as_str(),
            Some("2026-06-13T09:00:00Z")
        );
        assert_eq!(
            ev.payload["broadcaster"]["login"].as_str(),
            Some("host_chan")
        );
        assert_eq!(ev.payload["broadcaster"]["id"].as_str(), Some("100"));
    }

    #[tokio::test]
    async fn reward_redemption_event_publishes_documented_payload_shape() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "id": "redemption-42",
            "status": "unfulfilled",
            "user_input": "play my song",
            "redeemed_at": "2026-06-13T10:00:00Z",
            "user_id": "777",
            "user_login": "viewer_one",
            "user_name": "ViewerOne",
            "reward": {
                "id": "r1",
                "title": "Hydrate",
                "cost": 500,
                "prompt": "Make the streamer drink water"
            }
        });
        session.publish_reward_redemption_event(&event_data, "meta-redemption-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "channel.channel_points_redemption");
        assert_eq!(ev.source, EventSource::Twitch);
        assert_eq!(
            ev.payload["redemption"]["id"].as_str(),
            Some("redemption-42")
        );
        assert_eq!(ev.payload["reward"]["id"].as_str(), Some("r1"));
        assert_eq!(ev.payload["reward"]["title"].as_str(), Some("Hydrate"));
        assert_eq!(ev.payload["reward"]["cost"].as_i64(), Some(500));
    }

    #[tokio::test]
    async fn stream_offline_event_publishes_nested_broadcaster() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "broadcaster_user_id": "100",
            "broadcaster_user_login": "host_chan"
        });
        session.publish_stream_offline_event(&event_data, "meta-offline-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "stream.offline");
        assert_eq!(
            ev.payload["broadcaster"]["login"].as_str(),
            Some("host_chan")
        );
        assert_eq!(ev.payload["broadcaster"]["id"].as_str(), Some("100"));
    }
}
