use crate::bot_account::TwitchBotAccount;
use crate::chat::subscriber::{
    SubscribeError, SubscribePass, TopicRetries, TopicRetryRun, subscribe_all,
};
use crate::chat::topic_catalog::raid_direction;
use crate::config::{ChatSessionConfig, ChatTiming};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{FixedTokenSource, HelixHttpTransport, HelixTransport};
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::SubscriptionTracker;
use crate::vocabulary::{self, PlatformReauthRequired, RaidDirection};
use forge_events::{Event, EventPublisher, EventSource, TypedEvent};
use forge_net::{Backoff, Gateway};
use forge_platform_core::{
    ConnectionState, ConnectionTransition, EndpointSurface, PlatformEndpoints, PlatformError,
    connection_state_changed_event,
};
use forge_types::{
    AUTHOR_IS_CONNECTED_BOT_FIELD, ChatModerationAction, ChatModerationPayload, ChatPayload,
    ChatReply, ChatViewer, OAuthToken, UserRemoval,
};
use futures_util::{FutureExt, StreamExt};
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};
use tokio::time::{Duration, Instant, sleep_until};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, trace, warn};

use super::dedup::MessageIdWindow;
use super::dispatch;
use super::payload;
use crate::creator_goals::{GoalProgressOrigin, goal_progress_event};
use crate::vocabulary::events::automod::{
    AutomodMessageHeld, AutomodMessageUpdated, AutomodSettingsUpdated, AutomodTermsUpdated,
    BlockedTermMatch, HeldMessage, ReviewedMessage,
};
use crate::vocabulary::events::channel::{
    AdBreak, AdBreakBegan, ChannelInfo, ChannelUpdated, GuestStarGuest, GuestStarGuestUpdated,
    GuestStarHost, GuestStarSessionBegan, GuestStarSessionEnded, GuestStarSessionOpening,
    GuestStarSessionSpan, GuestStarSettings, GuestStarSettingsUpdated, LiveStream,
    ShoutoutReceived, ShoutoutSent, StreamWentOffline, StreamWentOnline, UpdatedUser, UserUpdated,
};
use crate::vocabulary::events::chat::{
    ChatAuthor, ChatCheer, ChatCleared, ChatMessageDeleted, ChatMessageReceived, ChatOrigin,
    ChatSettings, ChatSettingsUpdated, SharedChatBegan, SharedChatEnded, SharedChatSession,
    SharedChatState, SharedChatUpdated, WhisperBody, WhisperReceived, WhisperSender,
};
use crate::vocabulary::events::engagement::{
    GoalBegan, GoalClosing, GoalEnded, GoalOpening, PollBegan, PollChoice, PollClosing, PollEnded,
    PollHeading, PollOpening, PollProgressed, PredictionBegan, PredictionEnded, PredictionHeading,
    PredictionLock, PredictionLocked, PredictionOpening, PredictionOutcome, PredictionProgressed,
    PredictionResolution,
};
use crate::vocabulary::events::moderation::{
    ModeratorAdded, ModeratorRemoved, ShieldModeBegan, ShieldModeEnded, SuspiciousUserMessaged,
    UnbanRequestCreated, UnbanRequestResolved, UserBanned, UserUnbanned, VipAdded, VipRemoved,
    WarningAcknowledged, WarningSent,
};
use crate::vocabulary::events::parties::{TwitchBroadcaster, TwitchUser, UserRef};
use crate::vocabulary::events::rewards::{
    AutomaticRedemption, AutomaticReward, AutomaticRewardRedeemed, CustomReward, CustomRewardAdded,
    CustomRewardRemoved, CustomRewardState, CustomRewardUpdated, RedeemedReward, Redemption,
    RedemptionUpdated, RewardRedeemed, UpdatedRedemptionReward,
};
use crate::vocabulary::events::support::{
    BitsCheered, ChannelFollowed, ChannelRaided, ChannelResubscribed, ChannelSubscribed,
    CharityCampaignProgressed, CharityCampaignStarted, CharityCampaignState,
    CharityCampaignStopped, CharityCampaignTotals, CharityDonated, CharityDonation, HypeTrainBegan,
    HypeTrainClosing, HypeTrainEnded, HypeTrainOpening, HypeTrainProgress, HypeTrainProgressed,
    SubscriptionsGifted,
};
use crate::vocabulary::fields::automod as automod_fields;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SUCCESSOR_TIMEOUT: Duration = Duration::from_secs(15);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const PREDECESSOR_DRAIN_LIMIT: usize = 256;
const HELD_FRAME_LIMIT: usize = 1024;
const AUTHORIZATION_REVOKED_STATUS: &str = "authorization_revoked";

type EventSubSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

type SuccessorDial = Pin<Box<dyn Future<Output = Result<(EventSubSocket, String), String>> + Send>>;

type SubscriptionPassRun = Pin<Box<dyn Future<Output = PassVerdict> + Send>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatConnectionState {
    Connecting,
    Connected,
    Reconnecting { attempt: u8 },
    Disconnected,
}

impl ChatConnectionState {
    pub(crate) fn to_connection_state(self) -> ConnectionState {
        match self {
            Self::Connecting => ConnectionState::Connecting,
            Self::Connected => ConnectionState::Connected,
            Self::Reconnecting { .. } => ConnectionState::Reconnecting,
            Self::Disconnected => ConnectionState::Disconnected,
        }
    }
}

struct SessionConfig {
    manager: Arc<TwitchCredentialsManager>,
    helix: Gateway,
    client_id: String,
    broadcaster_id: String,
    user_id: String,
    endpoints: PlatformEndpoints,
    bus: Arc<dyn EventPublisher>,
    tracker: SubscriptionTracker,
    lifecycle: TwitchLifecycle,
    bot: Option<Arc<TwitchBotAccount>>,
}

pub(crate) struct ChatSession {
    config: SessionConfig,
    state_tx: watch::Sender<ChatConnectionState>,
    shutdown_rx: oneshot::Receiver<()>,
    transition: ConnectionTransition,
    seen_messages: MessageIdWindow,
    backoff: Backoff,
    keepalive_timeout: Duration,
    retries_rx: Option<oneshot::Receiver<TopicRetries>>,
}

impl ChatSession {
    pub(crate) fn new(
        manager: Arc<TwitchCredentialsManager>,
        helix: Gateway,
        config: ChatSessionConfig,
        bus: Arc<dyn EventPublisher>,
        tracker: SubscriptionTracker,
        lifecycle: TwitchLifecycle,
    ) -> (
        Self,
        watch::Receiver<ChatConnectionState>,
        oneshot::Sender<()>,
    ) {
        let (state_tx, state_rx) = watch::channel(ChatConnectionState::Connecting);
        let (session, shutdown_tx) =
            Self::reporting_to(manager, helix, config, bus, tracker, lifecycle, state_tx);
        (session, state_rx, shutdown_tx)
    }

    pub(crate) fn reporting_to(
        manager: Arc<TwitchCredentialsManager>,
        helix: Gateway,
        config: ChatSessionConfig,
        bus: Arc<dyn EventPublisher>,
        tracker: SubscriptionTracker,
        lifecycle: TwitchLifecycle,
        state_tx: watch::Sender<ChatConnectionState>,
    ) -> (Self, oneshot::Sender<()>) {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let ChatSessionConfig {
            client_id,
            broadcaster_id,
            user_id,
            endpoints,
        } = config;
        let session = Self {
            config: SessionConfig {
                manager,
                helix,
                client_id,
                broadcaster_id,
                user_id,
                endpoints,
                bus,
                tracker,
                lifecycle,
                bot: None,
            },
            state_tx,
            shutdown_rx,
            transition: ConnectionTransition::Unexpected,
            seen_messages: MessageIdWindow::default(),
            backoff: Backoff::default(),
            keepalive_timeout: crate::config::KEEPALIVE_TIMEOUT,
            retries_rx: None,
        };
        (session, shutdown_tx)
    }

    pub(crate) fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.config.bot = bot;
        self
    }

    pub(crate) fn with_timing(mut self, timing: ChatTiming) -> Self {
        self.keepalive_timeout = timing.keepalive_timeout;
        self.backoff = Backoff::new(timing.reconnect_backoff_base, timing.reconnect_backoff_cap);
        self
    }

    pub(crate) async fn run(mut self) {
        let url = self
            .config
            .endpoints
            .base_url(EndpointSurface::TwitchEventSubSocket)
            .to_owned();
        loop {
            let attempt = self.backoff.attempt();
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
                SessionOutcome::Disconnected => {}
                SessionOutcome::Shutdown => {
                    self.transition = ConnectionTransition::Expected;
                    break;
                }
                SessionOutcome::ReauthRequired => {
                    error!(
                        "twitch chat session stopped: re-authorization required (rejected \
                         refresh or missing scope); sign in again from the banner"
                    );
                    break;
                }
            }

            let retry_attempt = self.backoff.attempt();
            let delay = self.backoff.next_delay();
            info!(
                attempt = retry_attempt,
                retry_in_ms = delay.as_millis() as u64,
                "twitch chat session ended; reconnecting after backoff"
            );
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = &mut self.shutdown_rx => {
                    self.transition = ConnectionTransition::Expected;
                    break;
                }
            }
        }

        info!("twitch chat stopped");
    }

    async fn run_session(&mut self, url: &str) -> SessionOutcome {
        debug!("opening eventsub websocket");
        let connected = tokio::select! {
            connected = connect_socket(url) => connected,
            _ = &mut self.shutdown_rx => return SessionOutcome::Shutdown,
        };
        let mut live = match connected {
            Ok(socket) => Some(socket),
            Err(reason) => {
                warn!(error = %reason, "WebSocket connect failed");
                return SessionOutcome::Disconnected;
            }
        };

        debug!("eventsub websocket open; awaiting session_welcome");

        let mut session_id: Option<String> = None;
        let mut successor: Option<SuccessorDial> = None;
        let mut subscription_pass: Option<SubscriptionPassRun> = None;
        let mut topic_retries: Option<TopicRetryRun> = None;
        let mut held_frames: Vec<String> = Vec::new();
        let mut keepalive_deadline = Instant::now() + self.keepalive_timeout;

        loop {
            tokio::select! {
                _ = &mut self.shutdown_rx => {
                    if let Some(socket) = live.take() {
                        close_socket(socket).await;
                    }
                    debug!("twitch chat session shut down on request");
                    return SessionOutcome::Shutdown;
                }

                verdict = await_subscription_pass(&mut subscription_pass) => {
                    subscription_pass = None;
                    match verdict {
                        PassVerdict::Live => {
                            topic_retries = self
                                .retries_rx
                                .take()
                                .and_then(|mut handoff| handoff.try_recv().ok())
                                .and_then(TopicRetries::into_run);
                            self.backoff.reset();
                            self.set_state(ChatConnectionState::Connected);
                            self.publish_connection_event();
                            info!(broadcaster_id = %self.config.broadcaster_id, "chat connected");
                            for text in std::mem::take(&mut held_frames) {
                                match self.handle_frame(&text, &mut session_id).await {
                                    FrameAction::Continue | FrameAction::Subscribe(_) => {}
                                    FrameAction::Reconnect(new_url) => {
                                        arm_successor(&mut successor, new_url);
                                    }
                                    FrameAction::Disconnect => return SessionOutcome::Disconnected,
                                    FrameAction::ReauthRequired => {
                                        return SessionOutcome::ReauthRequired;
                                    }
                                }
                            }
                        }
                        PassVerdict::NothingLive => {
                            warn!("no eventsub subscription became active; treating as disconnect");
                            return SessionOutcome::Disconnected;
                        }
                        PassVerdict::TokenUnavailable => return SessionOutcome::Disconnected,
                        PassVerdict::ReauthRequired => {
                            self.publish_reauth_required();
                            return SessionOutcome::ReauthRequired;
                        }
                    }
                }

                () = await_topic_retries(&mut topic_retries) => {
                    topic_retries = None;
                }

                _ = sleep_until(keepalive_deadline), if live.is_some() => {
                    if successor.is_none() {
                        warn!("keepalive timeout; treating as disconnect");
                        return SessionOutcome::Disconnected;
                    }
                    debug!("predecessor socket went silent while the successor connects");
                    live = None;
                }

                dialed = await_successor(&mut successor) => {
                    successor = None;
                    match dialed {
                        Ok((socket, new_session_id)) => {
                            if let Some(mut predecessor) = live.take() {
                                let unfinished = self
                                    .drain_predecessor(&mut predecessor, &mut session_id)
                                    .await;
                                close_in_background(predecessor);
                                match unfinished {
                                    Some(FrameAction::Disconnect) => {
                                        close_in_background(socket);
                                        return SessionOutcome::Disconnected;
                                    }
                                    Some(FrameAction::ReauthRequired) => {
                                        close_in_background(socket);
                                        return SessionOutcome::ReauthRequired;
                                    }
                                    Some(
                                        FrameAction::Continue
                                        | FrameAction::Reconnect(_)
                                        | FrameAction::Subscribe(_),
                                    )
                                    | None => {}
                                }
                            }
                            if let Some(retries) = &topic_retries {
                                retries.move_to_session(&new_session_id);
                            }
                            let previous = session_id.replace(new_session_id);
                            debug!(
                                from_session = previous.as_deref().unwrap_or_default(),
                                "moved to the successor eventsub session"
                            );
                            live = Some(socket);
                            keepalive_deadline = Instant::now() + self.keepalive_timeout;
                        }
                        Err(reason) => {
                            warn!(error = %reason, "successor eventsub connection failed");
                            if live.is_none() {
                                return SessionOutcome::Disconnected;
                            }
                        }
                    }
                }

                msg = next_message(&mut live) => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            keepalive_deadline = Instant::now() + self.keepalive_timeout;
                            if subscription_pass.is_some() {
                                if held_frames.len() < HELD_FRAME_LIMIT {
                                    held_frames.push(text.to_string());
                                } else {
                                    debug!("frame arrived during a full subscription-pass hold; dropped");
                                }
                                continue;
                            }
                            match self.handle_frame(&text, &mut session_id).await {
                                FrameAction::Continue => {}
                                FrameAction::Reconnect(new_url) => {
                                    arm_successor(&mut successor, new_url);
                                }
                                FrameAction::Subscribe(pass) => {
                                    topic_retries = None;
                                    subscription_pass = Some(pass);
                                }
                                FrameAction::Disconnect => return SessionOutcome::Disconnected,
                                FrameAction::ReauthRequired => {
                                    return SessionOutcome::ReauthRequired;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            warn!(error = %ws_error_reason(&e), "WebSocket read error");
                            if successor.is_none() {
                                return SessionOutcome::Disconnected;
                            }
                            live = None;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            debug!("eventsub socket closed");
                            if successor.is_none() {
                                return SessionOutcome::Disconnected;
                            }
                            live = None;
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }
    }

    async fn drain_predecessor(
        &mut self,
        predecessor: &mut EventSubSocket,
        session_id: &mut Option<String>,
    ) -> Option<FrameAction> {
        for _ in 0..PREDECESSOR_DRAIN_LIMIT {
            let buffered = predecessor.next().now_or_never().flatten()?;
            match buffered {
                Ok(Message::Text(text)) => match self.handle_frame(&text, session_id).await {
                    FrameAction::Continue => {}
                    unfinished => return Some(unfinished),
                },
                Ok(_) => {}
                Err(e) => {
                    debug!(
                        error = %ws_error_reason(&e),
                        "predecessor read error while draining its buffered frames"
                    );
                    return None;
                }
            }
        }
        debug!("predecessor drain limit reached; remaining buffered frames dropped");
        None
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

                let manager = Arc::clone(&self.config.manager);
                let token_fetch =
                    tokio::spawn(async move { manager.get_valid_access_token().await });
                let fetched = match token_fetch.await {
                    Ok(fetched) => fetched,
                    Err(e) => {
                        warn!(error = %e, "chat token fetch task failed");
                        return FrameAction::Disconnect;
                    }
                };
                let token = match fetched {
                    Ok(token) => token,
                    Err(PlatformError::ReauthRequired { .. }) => {
                        warn!("chat token refresh rejected: reauth required");
                        self.publish_reauth_required();
                        return FrameAction::ReauthRequired;
                    }
                    Err(e) => {
                        warn_token_failure(&e);
                        return FrameAction::Disconnect;
                    }
                };

                let (retries_tx, retries_rx) = oneshot::channel();
                self.retries_rx = Some(retries_rx);
                let pass = SubscriptionPassContext {
                    manager: Arc::clone(&self.config.manager),
                    helix: self.config.helix.clone(),
                    client_id: self.config.client_id.clone(),
                    session_id: id,
                    broadcaster_id: self.config.broadcaster_id.clone(),
                    user_id: self.config.user_id.clone(),
                    bus: Arc::clone(&self.config.bus),
                    tracker: self.config.tracker.clone(),
                    retries_tx,
                };
                return FrameAction::Subscribe(Box::pin(pass.run(token)));
            }

            "session_keepalive" => {
                trace!("keepalive received");
            }

            "session_reconnect" => {
                let new_url = frame
                    .payload
                    .as_ref()
                    .and_then(|p| p.session.as_ref())
                    .and_then(|s| s.reconnect_url.clone());
                match new_url {
                    Some(new_url) => {
                        info!("server-initiated reconnect requested");
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
                if self.seen_messages.is_duplicate(frame_msg_id) {
                    debug!(subscription_type = %sub_type, "duplicate notification dropped");
                    return FrameAction::Continue;
                }
                if let Some(frame_payload) = &frame.payload
                    && let Some(event_data) = &frame_payload.event
                {
                    self.config.lifecycle.apply_notification(
                        sub_type,
                        event_data,
                        &self.config.broadcaster_id,
                    );
                    match dispatch::route_for(sub_type) {
                        Some(route) => route(self, event_data, frame_msg_id),
                        None => {
                            debug!(subscription_type = %sub_type, "no route registered for notification subscription type");
                        }
                    }
                }
            }

            "revocation" => {
                let Some(revoked) = frame.payload.as_ref().and_then(|p| p.subscription.as_ref())
                else {
                    warn!("revocation frame missing its subscription");
                    return FrameAction::Continue;
                };
                warn!(
                    subscription_type = %revoked.kind,
                    status = %revoked.status,
                    "eventsub subscription revoked by twitch"
                );
                self.config.tracker.mark_revoked(
                    &revoked.id,
                    &revoked.kind,
                    raid_direction(&revoked.condition),
                    &revoked.status,
                );
                if revoked.status == AUTHORIZATION_REVOKED_STATUS {
                    self.publish_reauth_required();
                    return FrameAction::ReauthRequired;
                }
            }

            other => {
                debug!(message_type = %other, "unhandled WS message type");
            }
        }

        FrameAction::Continue
    }

    pub(super) fn publish_chat_message(&self, event_data: &serde_json::Value) {
        let user_login = wire_text(event_data, "chatter_user_login");
        let user_id = wire_text(event_data, "chatter_user_id");
        let user_display = event_data
            .get("chatter_user_name")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(user_login.as_str())
            .to_owned();
        let roles = extract_roles_from_badges(event_data.get("badges"));
        let broadcaster_id = event_data
            .get("broadcaster_user_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        debug!(
            broadcaster_id = %broadcaster_id,
            user_id = %user_id,
            "chat message received"
        );

        let chat_payload = payload::build_chat_message_chat_payload(event_data);
        let cheer = event_data
            .get("cheer")
            .and_then(|c| c.get("bits"))
            .and_then(serde_json::Value::as_i64)
            .map(|bits| ChatCheer { bits });
        let source_id = event_data
            .get("source_broadcaster_user_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let from_channel = if !source_id.is_empty() && source_id != broadcaster_id {
            wire_non_empty(event_data, "source_broadcaster_user_login")
                .zip(wire_non_empty(event_data, "source_broadcaster_user_name"))
                .map(|(login, display_name)| ChatOrigin {
                    login: login.to_owned(),
                    display_name: display_name.to_owned(),
                })
        } else {
            None
        };
        let is_connected_bot = self
            .config
            .bot
            .as_ref()
            .and_then(|bot| bot.connected_user_id())
            .is_some_and(|bot_user_id| bot_user_id == user_id);
        let viewer = ChatViewer::new(user_id.clone(), user_login.clone());
        let reply = event_data
            .get("reply")
            .filter(|v| !v.is_null())
            .and_then(|reply| {
                let parent_author = reply.get("parent_user_name")?.as_str()?;
                let parent_text = reply.get("parent_message_body")?.as_str()?;
                Some(ChatReply {
                    parent_author: parent_author.to_owned(),
                    parent_text: parent_text.to_owned(),
                })
            });

        let message = ChatMessageReceived {
            channel: wire_text(event_data, "broadcaster_user_login"),
            user: ChatAuthor {
                login: user_login,
                id: user_id,
                display_name: user_display,
                roles: roles.clone(),
            },
            message: wire_text(wire_child(event_data, "message"), "text"),
            badges: roles,
            color: wire_text(event_data, "color"),
            cheer,
            from_channel,
        };
        let mut event = twitch_event(message);
        if is_connected_bot && let Some(fields) = event.payload.as_object_mut() {
            fields.insert(
                AUTHOR_IS_CONNECTED_BOT_FIELD.to_owned(),
                serde_json::Value::Bool(true),
            );
        }
        viewer.attach(&mut event.payload);
        attach_chat_payload(&mut event.payload, chat_payload);
        if let Some(reply) = reply {
            attach_chat_reply_payload(&mut event.payload, reply);
        }
        self.config.bus.publish(event);
    }

    pub(super) fn publish_subscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "subscriber event received");

        let chat_payload = payload::build_subscribe_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            ChannelSubscribed {
                user,
                tier: wire_text(event_data, "tier"),
                is_gift: wire_flag(event_data, "is_gift"),
            },
            chat_payload,
        );
    }

    pub(super) fn publish_resubscribe_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let cumulative_months = wire_count(event_data, "cumulative_months");

        debug!(user_id = %user.id, cumulative_months, "resub event received");

        let chat_payload = payload::build_resubscribe_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            ChannelResubscribed {
                user,
                tier: wire_text(event_data, "tier"),
                cumulative_months,
                streak_months: wire_count(event_data, "streak_months"),
                message: wire_text(wire_child(event_data, "message"), "text"),
                share_streak: wire_flag(event_data, "share_streak"),
            },
            chat_payload,
        );
    }

    pub(super) fn publish_gift_sub_event(
        &self,
        event_data: &serde_json::Value,
        frame_msg_id: &str,
    ) {
        let gifter = wire_user(event_data, &USER_KEYS);

        debug!(gifter_id = %gifter.id, "gift sub event received");

        let chat_payload = payload::build_gift_sub_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            SubscriptionsGifted {
                tier: wire_text(event_data, "tier"),
                is_anonymous: wire_flag(event_data, "is_anonymous"),
                gift_total: wire_count(event_data, "total"),
                gifter,
            },
            chat_payload,
        );
    }

    pub(super) fn publish_cheer_event(&self, event_data: &serde_json::Value, frame_msg_id: &str) {
        let user = wire_user(event_data, &USER_KEYS);
        let bits = wire_count(event_data, "bits");

        debug!(user_id = %user.id, bits, "cheer event received");

        let chat_payload = payload::build_cheer_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            BitsCheered {
                bits,
                message: wire_text(event_data, "message"),
                is_anonymous: wire_flag(event_data, "is_anonymous"),
                user,
            },
            chat_payload,
        );
    }

    pub(super) fn publish_raid_event(&self, event_data: &serde_json::Value, frame_msg_id: &str) {
        let from_broadcaster = wire_user(event_data, &FROM_BROADCASTER_KEYS);
        let to_broadcaster = wire_user(event_data, &TO_BROADCASTER_KEYS);
        let viewer_count = wire_count(event_data, "viewers");
        let direction = if to_broadcaster.id == self.config.broadcaster_id {
            RaidDirection::Received
        } else {
            RaidDirection::Sent
        };

        debug!(
            from_broadcaster_id = %from_broadcaster.id,
            viewer_count,
            direction = direction.as_str(),
            "raid event received"
        );

        let chat_payload = payload::build_raid_chat_payload(event_data, frame_msg_id);
        self.publish_with_chat(
            ChannelRaided {
                direction,
                viewer_count,
                from_broadcaster,
                to_broadcaster,
            },
            chat_payload,
        );
    }

    pub(super) fn publish_reward_redemption_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let reward = wire_child(event_data, "reward");
        let reward = RedeemedReward {
            id: wire_text(reward, "id"),
            title: wire_text(reward, "title"),
            cost: wire_count(reward, "cost"),
            prompt: wire_text(reward, "prompt"),
        };

        debug!(user_id = %user.id, reward_id = %reward.id, "channel point reward redemption received");

        self.publish_typed(RewardRedeemed {
            redemption: wire_redemption(event_data),
            user,
            reward,
        });
    }

    pub(super) fn publish_message_delete_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let message_id = wire_text(event_data, "message_id");
        let target_user = wire_user(event_data, &TARGET_USER_KEYS);

        debug!(target_user_id = %target_user.id, message_id = %message_id, "chat message deleted");

        self.publish_with_moderation(
            ChatMessageDeleted {
                message_id: message_id.clone(),
                target_user,
            },
            ChatModerationAction::DeleteMessage { message_id },
        );
    }

    pub(super) fn publish_chat_clear_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster = wire_broadcaster(event_data);

        debug!(broadcaster_id = %broadcaster.id, "chat cleared");

        self.publish_with_moderation(ChatCleared { broadcaster }, ChatModerationAction::ClearChat);
    }

    pub(super) fn publish_follow_event(&self, event_data: &serde_json::Value, _frame_msg_id: &str) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "follow event received");

        self.publish_typed(ChannelFollowed {
            followed_at: wire_text(event_data, "followed_at"),
            user,
        });
    }

    pub(super) fn publish_stream_online_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster = wire_broadcaster(event_data);

        debug!(broadcaster_id = %broadcaster.id, "stream online event received");

        self.publish_typed(StreamWentOnline {
            stream: LiveStream {
                id: wire_text(event_data, "id"),
                stream_type: wire_text(event_data, "type"),
                started_at: wire_text(event_data, "started_at"),
            },
            broadcaster,
        });
    }

    pub(super) fn publish_stream_offline_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let broadcaster = wire_broadcaster(event_data);

        debug!(broadcaster_id = %broadcaster.id, "stream offline event received");

        self.publish_typed(StreamWentOffline { broadcaster });
    }

    pub(super) fn publish_hype_train_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let hype = HypeTrainOpening {
            id: wire_text(event_data, "id"),
            level: wire_count(event_data, "level"),
            total: wire_count(event_data, "total"),
            goal: wire_count(event_data, "goal"),
            progress: wire_count(event_data, "progress"),
            started_at: wire_text(event_data, "started_at"),
            expires_at: wire_text(event_data, "expires_at"),
        };

        debug!(hype_id = %hype.id, level = hype.level, total = hype.total, "hype train began");

        self.publish_typed(HypeTrainBegan { hype });
    }

    pub(super) fn publish_hype_train_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let hype = HypeTrainProgress {
            id: wire_text(event_data, "id"),
            level: wire_count(event_data, "level"),
            total: wire_count(event_data, "total"),
            goal: wire_count(event_data, "goal"),
            progress: wire_count(event_data, "progress"),
        };

        debug!(hype_id = %hype.id, level = hype.level, progress = hype.progress, "hype train progressed");

        self.publish_typed(HypeTrainProgressed { hype });
    }

    pub(super) fn publish_hype_train_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let hype = HypeTrainClosing {
            id: wire_text(event_data, "id"),
            level: wire_count(event_data, "level"),
            total: wire_count(event_data, "total"),
            ended_at: wire_text(event_data, "ended_at"),
            cooldown_ends_at: wire_text(event_data, "cooldown_ends_at"),
        };

        debug!(hype_id = %hype.id, level = hype.level, total = hype.total, "hype train ended");

        self.publish_typed(HypeTrainEnded { hype });
    }

    pub(super) fn publish_charity_donation_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let amount = wire_child(event_data, "amount");
        let charity = CharityDonation {
            id: wire_text(event_data, "campaign_id"),
            name: wire_text(event_data, "charity_name"),
            description: wire_text(event_data, "charity_description"),
            website: wire_text(event_data, "charity_website"),
            amount_cents: wire_count(amount, "value"),
            currency_code: wire_text(amount, "currency"),
        };
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, amount_cents = charity.amount_cents, "charity donation received");

        self.publish_typed(CharityDonated { charity, user });
    }

    pub(super) fn publish_charity_start_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.publish_typed(CharityCampaignStarted(wire_charity_campaign(event_data)));
    }

    pub(super) fn publish_charity_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.publish_typed(CharityCampaignProgressed(wire_charity_campaign(event_data)));
    }

    pub(super) fn publish_charity_stop_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.publish_typed(CharityCampaignStopped(wire_charity_campaign(event_data)));
    }

    pub(super) fn publish_ban_event(&self, event_data: &serde_json::Value, _frame_msg_id: &str) {
        let user = wire_user(event_data, &USER_KEYS);
        let is_permanent = wire_flag(event_data, "is_permanent");

        debug!(
            user_id = %user.id,
            is_permanent,
            "ban event received"
        );

        let removal = ChatModerationAction::RemoveUser {
            user_id: (!user.id.is_empty()).then(|| user.id.clone()),
            user_name: user.display_name.clone(),
            removal: if is_permanent {
                UserRemoval::Ban
            } else {
                UserRemoval::Timeout
            },
        };
        self.publish_with_moderation(
            UserBanned {
                user,
                moderator: wire_user(event_data, &MODERATOR_KEYS),
                reason: wire_text(event_data, "reason"),
                banned_at: wire_text(event_data, "banned_at"),
                ends_at: event_data
                    .get("ends_at")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                is_permanent,
            },
            removal,
        );
    }

    pub(super) fn publish_unban_event(&self, event_data: &serde_json::Value, _frame_msg_id: &str) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "unban event received");

        self.publish_typed(UserUnbanned {
            user,
            moderator: wire_user(event_data, &MODERATOR_KEYS),
        });
    }

    pub(super) fn publish_moderator_add_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "moderator added");

        self.publish_typed(ModeratorAdded(UserRef { user }));
    }

    pub(super) fn publish_moderator_remove_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "moderator removed");

        self.publish_typed(ModeratorRemoved(UserRef { user }));
    }

    pub(super) fn publish_vip_add_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "vip added");

        self.publish_typed(VipAdded(UserRef { user }));
    }

    pub(super) fn publish_vip_remove_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "vip removed");

        self.publish_typed(VipRemoved(UserRef { user }));
    }

    pub(super) fn publish_unban_request_create_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let id = wire_text(event_data, "id");
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, request_id = %id, "unban request created");

        self.publish_typed(UnbanRequestCreated {
            id,
            user,
            reason_text: wire_text(event_data, "text"),
        });
    }

    pub(super) fn publish_unban_request_resolve_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let id = wire_text(event_data, "id");
        let user = wire_user(event_data, &USER_KEYS);
        let status = wire_text(event_data, "status");

        debug!(user_id = %user.id, request_id = %id, status = %status, "unban request resolved");

        self.publish_typed(UnbanRequestResolved {
            id,
            user,
            status,
            moderator: wire_user(event_data, &MODERATOR_KEYS),
            resolution_text: wire_text(event_data, "resolution_text"),
        });
    }

    pub(super) fn publish_shield_mode_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(moderator_id = %moderator.id, "shield mode started");

        self.publish_typed(ShieldModeBegan {
            moderator,
            started_at: wire_text(event_data, "started_at"),
        });
    }

    pub(super) fn publish_shield_mode_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(moderator_id = %moderator.id, "shield mode ended");

        self.publish_typed(ShieldModeEnded {
            moderator,
            ended_at: wire_text(event_data, "ended_at"),
        });
    }

    pub(super) fn publish_shoutout_create_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let to_broadcaster = wire_user(event_data, &TO_BROADCASTER_KEYS);
        let viewer_count = wire_count(event_data, "viewer_count");

        debug!(to_broadcaster_id = %to_broadcaster.id, viewer_count, "shoutout sent");

        self.publish_typed(ShoutoutSent {
            to_broadcaster,
            viewer_count,
            started_at: wire_text(event_data, "started_at"),
        });
    }

    pub(super) fn publish_shoutout_receive_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let from_broadcaster = wire_user(event_data, &FROM_BROADCASTER_KEYS);
        let viewer_count = wire_count(event_data, "viewer_count");

        debug!(from_broadcaster_id = %from_broadcaster.id, viewer_count, "shoutout received");

        self.publish_typed(ShoutoutReceived {
            from_broadcaster,
            viewer_count,
            started_at: wire_text(event_data, "started_at"),
        });
    }

    pub(super) fn publish_suspicious_user_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let low_trust_status = wire_text(event_data, "low_trust_status");

        debug!(user_id = %user.id, low_trust_status = %low_trust_status, "suspicious user message");

        self.publish_typed(SuspiciousUserMessaged {
            user,
            low_trust_status,
            message_text: wire_text(wire_child(event_data, "message"), "text"),
        });
    }

    pub(super) fn publish_warning_acknowledge_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "warning acknowledged");

        self.publish_typed(WarningAcknowledged(UserRef { user }));
    }

    pub(super) fn publish_warning_send_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(user_id = %user.id, moderator_id = %moderator.id, "warning sent");

        self.publish_typed(WarningSent {
            user,
            moderator,
            reason: wire_text(event_data, "reason"),
            chat_rules_cited: wire_list(event_data, "chat_rules_cited"),
        });
    }

    pub(super) fn publish_poll_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let poll = PollOpening {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            started_at: wire_text(event_data, "started_at"),
            ends_at: wire_text(event_data, "ends_at"),
        };
        let choices = extract_poll_choices(event_data);

        debug!(poll_id = %poll.id, choices = choices.len(), "poll began");

        self.publish_typed(PollBegan { poll, choices });
    }

    pub(super) fn publish_poll_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let poll = PollHeading {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
        };
        let choices = extract_poll_choices(event_data);

        debug!(poll_id = %poll.id, "poll progress");

        self.publish_typed(PollProgressed { poll, choices });
    }

    pub(super) fn publish_poll_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let poll = PollClosing {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            status: wire_text(event_data, "status"),
            ended_at: wire_text(event_data, "ended_at"),
        };
        let choices = extract_poll_choices(event_data);

        debug!(poll_id = %poll.id, status = %poll.status, "poll ended");

        self.publish_typed(PollEnded { poll, choices });
    }

    pub(super) fn publish_prediction_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionOpening {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            started_at: wire_text(event_data, "started_at"),
            locks_at: wire_text(event_data, "locks_at"),
        };
        let outcomes = extract_prediction_outcomes(event_data);

        debug!(prediction_id = %prediction.id, outcomes = outcomes.len(), "prediction began");

        self.publish_typed(PredictionBegan {
            prediction,
            outcomes,
        });
    }

    pub(super) fn publish_prediction_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionHeading {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
        };
        let outcomes = extract_prediction_outcomes(event_data);

        debug!(prediction_id = %prediction.id, "prediction progress");

        self.publish_typed(PredictionProgressed {
            prediction,
            outcomes,
        });
    }

    pub(super) fn publish_prediction_lock_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionLock {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            locked_at: wire_text(event_data, "locked_at"),
        };

        debug!(prediction_id = %prediction.id, "prediction locked");

        self.publish_typed(PredictionLocked { prediction });
    }

    pub(super) fn publish_prediction_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let prediction = PredictionResolution {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            winning_outcome_id: wire_text(event_data, "winning_outcome_id"),
            status: wire_text(event_data, "status"),
            ended_at: wire_text(event_data, "ended_at"),
        };

        debug!(prediction_id = %prediction.id, status = %prediction.status, "prediction ended");

        self.publish_typed(PredictionEnded { prediction });
    }

    pub(super) fn publish_goal_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let goal = GoalOpening {
            id: wire_text(event_data, "id"),
            goal_type: wire_text(event_data, "type"),
            description: wire_text(event_data, "description"),
            current_amount: wire_count(event_data, "current_amount"),
            target_amount: wire_count(event_data, "target_amount"),
            started_at: wire_text(event_data, "started_at"),
        };

        debug!(goal_id = %goal.id, goal_type = %goal.goal_type, "goal begun");

        self.publish_typed(GoalBegan { goal });
    }

    pub(super) fn publish_goal_progress_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        self.config
            .bus
            .publish(goal_progress_event(event_data, GoalProgressOrigin::Live));
    }

    pub(super) fn publish_goal_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let goal = GoalClosing {
            id: wire_text(event_data, "id"),
            goal_type: wire_text(event_data, "type"),
            current_amount: wire_count(event_data, "current_amount"),
            target_amount: wire_count(event_data, "target_amount"),
            is_achieved: wire_flag(event_data, "is_achieved"),
            ended_at: wire_text(event_data, "ended_at"),
        };

        debug!(goal_id = %goal.id, is_achieved = goal.is_achieved, "goal ended");

        self.publish_typed(GoalEnded { goal });
    }

    pub(super) fn publish_reward_add_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_custom_reward(event_data);

        debug!(reward_id = %state.reward.id, "channel point reward added");

        self.publish_typed(CustomRewardAdded(state));
    }

    pub(super) fn publish_reward_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_custom_reward(event_data);

        debug!(reward_id = %state.reward.id, "channel point reward updated");

        self.publish_typed(CustomRewardUpdated(state));
    }

    pub(super) fn publish_reward_remove_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_custom_reward(event_data);

        debug!(reward_id = %state.reward.id, "channel point reward removed");

        self.publish_typed(CustomRewardRemoved(state));
    }

    pub(super) fn publish_redemption_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let redemption = wire_redemption(event_data);
        let user = wire_user(event_data, &USER_KEYS);
        let reward = wire_child(event_data, "reward");
        let reward = UpdatedRedemptionReward {
            id: wire_text(reward, "id"),
            title: wire_text(reward, "title"),
            cost: wire_count(reward, "cost"),
        };

        debug!(user_id = %user.id, reward_id = %reward.id, status = %redemption.status, "channel point redemption updated");

        self.publish_typed(RedemptionUpdated {
            redemption,
            user,
            reward,
        });
    }

    pub(super) fn publish_automod_hold_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let reason = wire_text(event_data, automod_fields::REASON);
        let classification = wire_automod_classification(event_data, &reason);

        debug!(
            user_id = %user.id,
            reason = %reason,
            category = %classification.category,
            level = classification.level,
            "automod hold received"
        );

        self.publish_typed(AutomodMessageHeld {
            automod: HeldMessage {
                message_id: wire_text(event_data, "message_id"),
                category: classification.category_unless_blocked_term(),
                level: classification.level_unless_blocked_term(),
                held_at: wire_text(event_data, "held_at"),
            },
            reason,
            blocked_term: classification.blocked_term_match(),
            user,
            message_text: wire_text(wire_child(event_data, "message"), "text"),
        });
    }

    pub(super) fn publish_chat_settings_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let settings = ChatSettings {
            emote_mode: wire_flag(event_data, "emote_mode"),
            follower_mode: wire_flag(event_data, "follower_mode"),
            follower_mode_duration_minutes: wire_optional_count(
                event_data,
                "follower_mode_duration_minutes",
            ),
            slow_mode: wire_flag(event_data, "slow_mode"),
            slow_mode_wait_time_seconds: wire_optional_count(
                event_data,
                "slow_mode_wait_time_seconds",
            ),
            subscriber_mode: wire_flag(event_data, "subscriber_mode"),
            unique_chat_mode: wire_flag(event_data, "unique_chat_mode"),
        };

        debug!(
            emote_mode = settings.emote_mode,
            follower_mode = settings.follower_mode,
            slow_mode = settings.slow_mode,
            subscriber_mode = settings.subscriber_mode,
            unique_chat_mode = settings.unique_chat_mode,
            "chat settings updated"
        );

        self.publish_typed(ChatSettingsUpdated { settings });
    }

    pub(super) fn publish_guest_star_session_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let session = GuestStarSessionOpening {
            id: wire_text(event_data, "session_id"),
            started_at: wire_text(event_data, "started_at"),
        };

        debug!(session_id = %session.id, "guest star session began");

        self.publish_typed(GuestStarSessionBegan { session });
    }

    pub(super) fn publish_guest_star_session_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let session = GuestStarSessionSpan {
            id: wire_text(event_data, "session_id"),
            started_at: wire_text(event_data, "started_at"),
            ended_at: wire_text(event_data, "ended_at"),
        };

        debug!(session_id = %session.id, "guest star session ended");

        self.publish_typed(GuestStarSessionEnded { session });
    }

    pub(super) fn publish_guest_star_settings_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let settings = GuestStarSettings {
            slot_count: wire_count(event_data, "slot_count"),
            group_layout: wire_text(event_data, "group_layout"),
            is_moderator_send_live_enabled: wire_flag(event_data, "is_moderator_send_live_enabled"),
            is_browser_source_audio_enabled: wire_flag(
                event_data,
                "is_browser_source_audio_enabled",
            ),
        };

        debug!(slot_count = settings.slot_count, group_layout = %settings.group_layout, "guest star settings updated");

        self.publish_typed(GuestStarSettingsUpdated { settings });
    }

    pub(super) fn publish_guest_star_guest_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let session_id = wire_text(event_data, "session_id");
        let state = wire_text(event_data, "state");
        let guest = GuestStarGuest {
            id: wire_non_empty(event_data, "guest_user_id").map(str::to_owned),
            login: wire_non_empty(event_data, "guest_user_login").map(str::to_owned),
            display_name: wire_non_empty(event_data, "guest_user_name").map(str::to_owned),
        };

        debug!(
            session_id = %session_id,
            guest_user_id = %guest.id.as_deref().unwrap_or_default(),
            state = %state,
            "guest star guest update"
        );

        self.publish_typed(GuestStarGuestUpdated {
            session_id,
            slot_id: wire_text(event_data, "slot_id"),
            state,
            guest,
            host: GuestStarHost {
                video_enabled: wire_flag(event_data, "host_video_enabled"),
                audio_enabled: wire_flag(event_data, "host_audio_enabled"),
                volume: wire_count(event_data, "host_volume"),
            },
        });
    }

    pub(super) fn publish_automod_settings_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);

        debug!(moderator_id = %moderator.id, "automod settings updated");

        self.publish_typed(AutomodSettingsUpdated {
            moderator,
            overall_level: wire_optional_count(event_data, "overall_level"),
        });
    }

    pub(super) fn publish_automod_terms_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let moderator = wire_user(event_data, &MODERATOR_KEYS);
        let action = wire_text(event_data, "action");

        debug!(moderator_id = %moderator.id, action = %action, "automod terms updated");

        self.publish_typed(AutomodTermsUpdated {
            moderator,
            action,
            terms: wire_list(event_data, "terms"),
        });
    }

    pub(super) fn publish_automod_message_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let status = wire_text(event_data, "status");
        let reason = wire_text(event_data, automod_fields::REASON);
        let classification = wire_automod_classification(event_data, &reason);

        debug!(
            user_id = %user.id,
            status = %status,
            reason = %reason,
            category = %classification.category,
            "automod message updated"
        );

        self.publish_typed(AutomodMessageUpdated {
            automod: ReviewedMessage {
                message_id: wire_text(event_data, "message_id"),
                status,
                category: classification.category_unless_blocked_term(),
                level: classification.level_unless_blocked_term(),
            },
            reason,
            blocked_term: classification.blocked_term_match(),
            user,
            moderator: wire_user(event_data, &MODERATOR_KEYS),
            message_text: wire_text(wire_child(event_data, "message"), "text"),
        });
    }

    pub(super) fn publish_shared_chat_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_shared_chat(event_data);

        debug!(session_id = %state.shared_chat.session_id, host_id = %state.host.id, "shared chat session began");

        self.publish_typed(SharedChatBegan(state));
    }

    pub(super) fn publish_shared_chat_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_shared_chat(event_data);

        debug!(session_id = %state.shared_chat.session_id, host_id = %state.host.id, "shared chat session updated");

        self.publish_typed(SharedChatUpdated(state));
    }

    pub(super) fn publish_shared_chat_end_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let state = wire_shared_chat(event_data);

        debug!(session_id = %state.shared_chat.session_id, host_id = %state.host.id, "shared chat session ended");

        self.publish_typed(SharedChatEnded(state));
    }

    pub(super) fn publish_channel_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let channel = ChannelInfo {
            title: wire_text(event_data, "title"),
            language: wire_text(event_data, "language"),
            category_id: wire_text(event_data, "category_id"),
            category_name: wire_text(event_data, "category_name"),
        };

        debug!(category_id = %channel.category_id, "channel update event received");

        self.publish_typed(ChannelUpdated { channel });
    }

    pub(super) fn publish_ad_break_begin_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let ad_break = AdBreak {
            duration_seconds: wire_count(event_data, "duration_seconds"),
            is_automatic: wire_flag(event_data, "is_automatic"),
            started_at: wire_text(event_data, "started_at"),
        };

        debug!(
            duration_seconds = ad_break.duration_seconds,
            is_automatic = ad_break.is_automatic,
            "ad break begin event received"
        );

        self.publish_typed(AdBreakBegan {
            ad_break,
            requester: wire_user(event_data, &REQUESTER_KEYS),
        });
    }

    pub(super) fn publish_automatic_reward_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);
        let reward = wire_child(event_data, "reward");
        let reward = AutomaticReward {
            reward_type: wire_text(reward, "type"),
            cost: wire_count(reward, "cost"),
        };

        debug!(user_id = %user.id, reward_type = %reward.reward_type, "automatic channel point reward redeemed");

        self.publish_typed(AutomaticRewardRedeemed {
            redemption: AutomaticRedemption {
                id: wire_text(event_data, "id"),
                redeemed_at: wire_text(event_data, "redeemed_at"),
            },
            user,
            reward,
        });
    }

    pub(super) fn publish_whisper_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let sender = wire_user(event_data, &WHISPER_SENDER_KEYS);

        debug!(from_user_id = %sender.id, "whisper received");

        self.publish_typed(WhisperReceived {
            user: WhisperSender {
                id: sender.id,
                login: sender.login,
                display_name: sender.display_name,
                color: None,
            },
            whisper: WhisperBody {
                text: wire_text(wire_child(event_data, "whisper"), "text"),
            },
            whisper_thread_id: wire_text(event_data, "whisper_id"),
        });
    }

    pub(super) fn publish_user_update_event(
        &self,
        event_data: &serde_json::Value,
        _frame_msg_id: &str,
    ) {
        let user = wire_user(event_data, &USER_KEYS);

        debug!(user_id = %user.id, "user profile updated");

        self.publish_typed(UserUpdated {
            user: UpdatedUser {
                id: user.id,
                login: user.login,
                display_name: user.display_name,
                description: wire_text(event_data, "description"),
            },
        });
    }

    fn publish_typed(&self, payload: impl TypedEvent) {
        payload.publish(self.config.bus.as_ref(), EventSource::Twitch, None);
    }

    fn publish_with_chat(&self, payload: impl TypedEvent, chat: ChatPayload) {
        let mut event = twitch_event(payload);
        attach_chat_payload(&mut event.payload, chat);
        self.config.bus.publish(event);
    }

    fn publish_with_moderation(&self, payload: impl TypedEvent, action: ChatModerationAction) {
        let mut event = twitch_event(payload);
        attach_moderation_payload(&mut event.payload, action);
        self.config.bus.publish(event);
    }

    fn set_state(&self, state: ChatConnectionState) {
        self.state_tx.send_replace(state);
    }

    fn publish_connection_event(&self) {
        let state = self.state_tx.borrow().to_connection_state();
        self.config.bus.publish(connection_state_changed_event(
            vocabulary::PLATFORM_ID,
            state,
            ConnectionTransition::Unexpected,
        ));
    }

    fn publish_reauth_required(&self) {
        PlatformReauthRequired {
            platform_id: vocabulary::PLATFORM,
        }
        .publish(self.config.bus.as_ref(), EventSource::Twitch, None);
    }
}

fn twitch_event(payload: impl TypedEvent) -> Event {
    payload.into_event(EventSource::Twitch, None)
}

struct WireUserKeys {
    id: &'static str,
    login: &'static str,
    name: &'static str,
}

const USER_KEYS: WireUserKeys = WireUserKeys {
    id: "user_id",
    login: "user_login",
    name: "user_name",
};

const MODERATOR_KEYS: WireUserKeys = WireUserKeys {
    id: "moderator_user_id",
    login: "moderator_user_login",
    name: "moderator_user_name",
};

const TARGET_USER_KEYS: WireUserKeys = WireUserKeys {
    id: "target_user_id",
    login: "target_user_login",
    name: "target_user_name",
};

const FROM_BROADCASTER_KEYS: WireUserKeys = WireUserKeys {
    id: "from_broadcaster_user_id",
    login: "from_broadcaster_user_login",
    name: "from_broadcaster_user_name",
};

const TO_BROADCASTER_KEYS: WireUserKeys = WireUserKeys {
    id: "to_broadcaster_user_id",
    login: "to_broadcaster_user_login",
    name: "to_broadcaster_user_name",
};

const HOST_BROADCASTER_KEYS: WireUserKeys = WireUserKeys {
    id: "host_broadcaster_user_id",
    login: "host_broadcaster_user_login",
    name: "host_broadcaster_user_name",
};

const REQUESTER_KEYS: WireUserKeys = WireUserKeys {
    id: "requester_user_id",
    login: "requester_user_login",
    name: "requester_user_name",
};

const WHISPER_SENDER_KEYS: WireUserKeys = WireUserKeys {
    id: "from_user_id",
    login: "from_user_login",
    name: "from_user_name",
};

const BLOCKED_TERM_REASON: &str = "blocked_term";

static WIRE_ABSENT: serde_json::Value = serde_json::Value::Null;

fn wire_child<'a>(data: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    data.get(key).unwrap_or(&WIRE_ABSENT)
}

fn wire_text(data: &serde_json::Value, key: &str) -> String {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn wire_non_empty<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
}

fn wire_count(data: &serde_json::Value, key: &str) -> i64 {
    wire_optional_count(data, key).unwrap_or_default()
}

fn wire_optional_count(data: &serde_json::Value, key: &str) -> Option<i64> {
    data.get(key).and_then(serde_json::Value::as_i64)
}

fn wire_flag(data: &serde_json::Value, key: &str) -> bool {
    data.get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_default()
}

fn wire_list(data: &serde_json::Value, key: &str) -> Vec<serde_json::Value> {
    data.get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn wire_user(data: &serde_json::Value, keys: &WireUserKeys) -> TwitchUser {
    TwitchUser {
        id: wire_text(data, keys.id),
        login: wire_text(data, keys.login),
        display_name: wire_text(data, keys.name),
    }
}

fn wire_broadcaster(data: &serde_json::Value) -> TwitchBroadcaster {
    TwitchBroadcaster {
        id: wire_text(data, "broadcaster_user_id"),
        login: wire_text(data, "broadcaster_user_login"),
    }
}

fn wire_redemption(event_data: &serde_json::Value) -> Redemption {
    Redemption {
        id: wire_text(event_data, "id"),
        status: wire_text(event_data, "status"),
        user_input: wire_text(event_data, "user_input"),
        redeemed_at: wire_text(event_data, "redeemed_at"),
    }
}

fn wire_custom_reward(event_data: &serde_json::Value) -> CustomRewardState {
    CustomRewardState {
        reward: CustomReward {
            id: wire_text(event_data, "id"),
            title: wire_text(event_data, "title"),
            cost: wire_count(event_data, "cost"),
            prompt: wire_text(event_data, "prompt"),
            is_enabled: wire_flag(event_data, "is_enabled"),
        },
    }
}

fn wire_shared_chat(event_data: &serde_json::Value) -> SharedChatState {
    SharedChatState {
        shared_chat: SharedChatSession {
            session_id: wire_text(event_data, "session_id"),
        },
        host: wire_user(event_data, &HOST_BROADCASTER_KEYS),
    }
}

fn wire_charity_campaign(event_data: &serde_json::Value) -> CharityCampaignState {
    let current = wire_child(event_data, "current_amount");
    CharityCampaignState {
        charity: CharityCampaignTotals {
            id: wire_text(event_data, "id"),
            name: wire_text(event_data, "charity_name"),
            current_amount_cents: wire_count(current, "value"),
            target_amount_cents: wire_count(wire_child(event_data, "target_amount"), "value"),
            currency_code: wire_text(current, "currency"),
        },
    }
}

struct AutomodClassification {
    category: String,
    level: i64,
    is_blocked_term: bool,
    terms_found: Vec<String>,
}

impl AutomodClassification {
    fn category_unless_blocked_term(&self) -> Option<String> {
        (!self.is_blocked_term).then(|| self.category.clone())
    }

    fn level_unless_blocked_term(&self) -> Option<i64> {
        (!self.is_blocked_term).then_some(self.level)
    }

    fn blocked_term_match(&self) -> Option<BlockedTermMatch> {
        self.is_blocked_term.then(|| BlockedTermMatch {
            terms_found: self.terms_found.clone(),
        })
    }
}

fn wire_automod_classification(
    event_data: &serde_json::Value,
    reason: &str,
) -> AutomodClassification {
    let automod = wire_child(event_data, "automod");
    AutomodClassification {
        category: wire_text(automod, "category"),
        level: wire_count(automod, "level"),
        is_blocked_term: reason == BLOCKED_TERM_REASON,
        terms_found: blocked_terms_found(event_data),
    }
}

fn extract_poll_choices(event_data: &serde_json::Value) -> Vec<PollChoice> {
    event_data
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .map(|choice| PollChoice {
                    id: wire_text(choice, "id"),
                    title: wire_text(choice, "title"),
                    votes: wire_optional_count(choice, "votes"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_prediction_outcomes(event_data: &serde_json::Value) -> Vec<PredictionOutcome> {
    event_data
        .get("outcomes")
        .and_then(serde_json::Value::as_array)
        .map(|outcomes| {
            outcomes
                .iter()
                .map(|outcome| PredictionOutcome {
                    id: wire_text(outcome, "id"),
                    title: wire_text(outcome, "title"),
                    color: wire_text(outcome, "color"),
                    users: wire_optional_count(outcome, "users"),
                    channel_points: wire_optional_count(outcome, "channel_points"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn blocked_terms_found(event_data: &serde_json::Value) -> Vec<String> {
    event_data
        .get(automod_fields::BLOCKED_TERM)
        .and_then(|b| b.get(automod_fields::TERMS_FOUND))
        .and_then(|v| v.as_array())
        .map(|terms| {
            terms
                .iter()
                .filter_map(|t| t.get(automod_fields::TERM_ID))
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
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

fn attach_moderation_payload(forge_payload: &mut serde_json::Value, action: ChatModerationAction) {
    match serde_json::to_value(&ChatModerationPayload { action }) {
        Ok(mod_value) => {
            if let serde_json::Value::Object(map) = forge_payload {
                map.insert(ChatModerationPayload::KEY.to_owned(), mod_value);
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to serialize ChatModerationPayload; _chat_mod key omitted");
        }
    }
}

fn attach_chat_reply_payload(forge_payload: &mut serde_json::Value, reply: ChatReply) {
    match serde_json::to_value(&reply) {
        Ok(reply_value) => {
            if let serde_json::Value::Object(map) = forge_payload {
                map.insert(ChatReply::KEY.to_owned(), reply_value);
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to serialize ChatReply; _chat_reply key omitted");
        }
    }
}

fn ws_error_reason(e: &tokio_tungstenite::tungstenite::Error) -> String {
    match e {
        tokio_tungstenite::tungstenite::Error::Url(_) => "websocket url rejected".to_owned(),
        other => other.to_string(),
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

async fn connect_socket(url: &str) -> Result<EventSubSocket, String> {
    match tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((socket, _))) => Ok(socket),
        Ok(Err(e)) => Err(ws_error_reason(&e)),
        Err(_) => Err("connect timed out".to_owned()),
    }
}

async fn next_message(
    socket: &mut Option<EventSubSocket>,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    match socket {
        Some(socket) => socket.next().await,
        None => std::future::pending().await,
    }
}

async fn await_successor(
    dial: &mut Option<SuccessorDial>,
) -> Result<(EventSubSocket, String), String> {
    match dial {
        Some(dial) => dial.await,
        None => std::future::pending().await,
    }
}

async fn dial_successor(url: String) -> Result<(EventSubSocket, String), String> {
    match tokio::time::timeout(SUCCESSOR_TIMEOUT, welcome_on_successor(url)).await {
        Ok(welcomed) => welcomed,
        Err(_) => Err("timed out before session_welcome".to_owned()),
    }
}

async fn welcome_on_successor(url: String) -> Result<(EventSubSocket, String), String> {
    let mut socket = connect_socket(&url).await?;

    while let Some(frame) = socket.next().await {
        let Message::Text(text) = frame.map_err(|e| ws_error_reason(&e))? else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<WsFrame>(&text) else {
            continue;
        };
        if parsed.metadata.message_type != "session_welcome" {
            debug!(
                message_type = %parsed.metadata.message_type,
                "successor socket frame ahead of session_welcome"
            );
            continue;
        }
        return match parsed.payload.and_then(|payload| payload.session) {
            Some(session) => Ok((socket, session.id)),
            None => Err("session_welcome missing session.id".to_owned()),
        };
    }

    Err("closed before session_welcome".to_owned())
}

fn arm_successor(successor: &mut Option<SuccessorDial>, url: String) {
    if successor.is_some() {
        debug!("successor already dialing; repeated reconnect ignored");
    } else {
        *successor = Some(Box::pin(dial_successor(url)));
    }
}

async fn await_subscription_pass(pass: &mut Option<SubscriptionPassRun>) -> PassVerdict {
    match pass {
        Some(pass) => pass.await,
        None => std::future::pending().await,
    }
}

async fn await_topic_retries(retries: &mut Option<TopicRetryRun>) {
    match retries {
        Some(retries) => retries.finished().await,
        None => std::future::pending().await,
    }
}

async fn close_socket(mut socket: EventSubSocket) {
    if tokio::time::timeout(CLOSE_TIMEOUT, socket.close(None))
        .await
        .is_err()
    {
        tracing::debug!(timeout = ?CLOSE_TIMEOUT, "eventsub socket close timed out");
    }
}

fn close_in_background(socket: EventSubSocket) {
    tokio::spawn(close_socket(socket));
}

fn warn_token_failure(e: &PlatformError) {
    match e {
        PlatformError::Http { status, .. } => {
            warn!(
                status = *status,
                "chat token fetch failed; treating as disconnect"
            );
        }
        PlatformError::Network { reason } => {
            warn!(
                error = %reason,
                "chat token fetch failed; treating as disconnect"
            );
        }
        _ => warn!("chat token fetch failed; treating as disconnect"),
    }
}

struct SubscriptionPassContext {
    manager: Arc<TwitchCredentialsManager>,
    helix: Gateway,
    client_id: String,
    session_id: String,
    broadcaster_id: String,
    user_id: String,
    bus: Arc<dyn EventPublisher>,
    tracker: SubscriptionTracker,
    retries_tx: oneshot::Sender<TopicRetries>,
}

impl SubscriptionPassContext {
    async fn run(self, token: OAuthToken) -> PassVerdict {
        let result = match self.subscribe(&token).await {
            Err(SubscribeError::Unauthorized) => {
                debug!("eventsub subscription rejected the access token; refreshing once");
                match self.manager.refresh(&token).await {
                    Ok(renewed) => self.subscribe(&renewed.access_token).await,
                    Err(PlatformError::ReauthRequired { .. }) => {
                        warn!("chat token refresh rejected: reauth required");
                        return PassVerdict::ReauthRequired;
                    }
                    Err(e) => {
                        warn_token_failure(&e);
                        return PassVerdict::TokenUnavailable;
                    }
                }
            }
            first => first,
        };
        match result {
            Ok(pass) if pass.any_live() => {
                if pass.has_scope_gap() {
                    warn!(
                        "some eventsub topics were rejected for a missing scope; chat stays live"
                    );
                    PlatformReauthRequired {
                        platform_id: vocabulary::PLATFORM,
                    }
                    .publish(self.bus.as_ref(), EventSource::Twitch, None);
                }
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the session may stop waiting for the retry pass"
                )]
                let _ = self.retries_tx.send(pass.into_retries());
                PassVerdict::Live
            }
            Ok(_) => PassVerdict::NothingLive,
            Err(SubscribeError::ScopeMissing) => {
                warn!("chat subscription rejected: scope missing (reauth required)");
                PassVerdict::ReauthRequired
            }
            Err(SubscribeError::Unauthorized) => {
                warn!("chat subscription rejected the renewed token (reauth required)");
                PassVerdict::ReauthRequired
            }
        }
    }

    async fn subscribe(&self, token: &OAuthToken) -> Result<SubscribePass, SubscribeError> {
        let helix: Arc<dyn HelixTransport> = Arc::new(HelixHttpTransport::new(
            self.helix.clone(),
            self.client_id.clone(),
            Arc::new(FixedTokenSource::new(token.clone())),
        ));
        subscribe_all(
            &helix,
            &self.session_id,
            &self.broadcaster_id,
            &self.user_id,
            &self.bus,
            &self.tracker,
        )
        .await
    }
}

enum PassVerdict {
    Live,
    NothingLive,
    TokenUnavailable,
    ReauthRequired,
}

enum SessionOutcome {
    Disconnected,
    ReauthRequired,
    Shutdown,
}

enum FrameAction {
    Continue,
    Reconnect(String),
    Subscribe(SubscriptionPassRun),
    Disconnect,
    ReauthRequired,
}

impl Drop for ChatSession {
    fn drop(&mut self) {
        let transition = match self.shutdown_rx.try_recv() {
            Ok(()) => ConnectionTransition::Expected,
            Err(_) => self.transition,
        };
        self.state_tx
            .send_replace(ChatConnectionState::Disconnected);
        self.config.bus.publish(connection_state_changed_event(
            vocabulary::PLATFORM_ID,
            ConnectionState::Disconnected,
            transition,
        ));
    }
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
    subscription: Option<RevokedSubscription>,
    event: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RevokedSubscription {
    #[serde(default)]
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    condition: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct SessionInfo {
    id: String,
    reconnect_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::chat_platform::EVENT_CHANNEL_CAPACITY;
    use crate::sub_actions::test_support::MockCreds;
    use forge_events::EventChannel;
    use forge_events::{EventSource, EventSubscription};
    use forge_events::{PlatformConnectionChanged, TypedEvent};
    use forge_types::{ChatEventDetail, ChatPayload, ChatSegment};
    use tokio::sync::mpsc;

    use super::*;

    mod typed_payloads;

    fn make_session(bus: &Arc<EventChannel>) -> ChatSession {
        session_over_creds(bus, Arc::new(MockCreds::with_identity()))
    }

    fn session_config(endpoints: PlatformEndpoints) -> ChatSessionConfig {
        ChatSessionConfig {
            client_id: "client".to_owned(),
            broadcaster_id: "bcast".to_owned(),
            user_id: "user".to_owned(),
            endpoints,
        }
    }

    #[test]
    fn websocket_url_error_is_reported_without_the_connect_url() {
        const SESSION_URL: &str =
            "wss://eventsub.wss.twitch.tv/ws?reconnect_token=SESSION_SCOPED_SENTINEL_j7x";
        let err = tokio_tungstenite::tungstenite::Error::Url(
            tokio_tungstenite::tungstenite::error::UrlError::UnableToConnect(
                SESSION_URL.to_owned(),
            ),
        );
        assert!(
            err.to_string().contains(SESSION_URL),
            "fixture must reproduce the leak the mapper exists to prevent; \
             tungstenite Display changed: {err}"
        );

        let reason = ws_error_reason(&err);

        assert!(
            !reason.contains("SESSION_SCOPED_SENTINEL_j7x"),
            "url-error reason must not carry the connect URL; got: {reason}"
        );
    }

    #[test]
    fn non_url_websocket_error_keeps_its_diagnostic_detail() {
        let err = tokio_tungstenite::tungstenite::Error::Utf8("bad frame at byte 12".to_owned());

        let reason = ws_error_reason(&err);

        assert!(
            reason.contains("bad frame at byte 12"),
            "non-url variants must pass Display through so operators keep the detail; got: {reason}"
        );
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
    async fn publish_chat_message_emits_the_documented_payload_shape() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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

        assert_eq!(ev.kind, "twitch.channel.chat.message");
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
    async fn chat_message_is_flagged_as_the_connected_bot_only_for_the_bots_chatter_id() {
        use crate::bot_account::test_support::{BOT_USER_ID, BotHarness};

        let harness = BotHarness::connected().await;
        for (chatter_id, expected) in [(BOT_USER_ID, Some(true)), ("67890", None), ("", None)] {
            let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
            let session = make_session(&bus).with_bot(Some(Arc::clone(&harness.bot)));
            let mut sub = bus.subscribe();

            session.publish_chat_message(&serde_json::json!({
                "broadcaster_user_login": "streamer_channel",
                "chatter_user_id": chatter_id,
                "chatter_user_login": "someone",
                "message": {"text": "hi"},
            }));
            let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
                .await
                .expect("timeout")
                .expect("recv error");

            assert_eq!(
                ev.payload
                    .get(AUTHOR_IS_CONNECTED_BOT_FIELD)
                    .and_then(serde_json::Value::as_bool),
                expected,
                "chatter {chatter_id:?}"
            );
        }
    }

    #[tokio::test]
    async fn publish_chat_message_surfaces_cheer_bits_without_anonymity_signal() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

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
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
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

        assert_eq!(ev.kind, "twitch.channel.subscribe");
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

    async fn published(mut sub: EventSubscription) -> Event {
        tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("the ingest must publish within the wait")
            .expect("the bus must stay open")
    }

    #[tokio::test]
    async fn a_chat_message_carries_the_chatters_display_name_into_the_bus_payload() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let sub = bus.subscribe();

        session.publish_chat_message(&serde_json::json!({
            "broadcaster_user_login": "streamer",
            "chatter_user_id": "222",
            "chatter_user_login": "loyalfan",
            "chatter_user_name": "LoyalFan",
            "message": { "text": "hello there" },
        }));

        let event = published(sub).await;
        assert_eq!(
            event.payload["user"]["display_name"].as_str(),
            Some("LoyalFan")
        );
    }

    #[tokio::test]
    async fn every_chat_line_trigger_publishes_the_eventsub_message_id_of_the_ingested_line() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let sub = bus.subscribe();
        session.publish_chat_message(&serde_json::json!({
            "broadcaster_user_id": "bcast",
            "broadcaster_user_login": "streamer",
            "chatter_user_id": "222",
            "chatter_user_login": "loyalfan",
            "chatter_user_name": "LoyalFan",
            "message_id": "cc106a89-1814-919d-454c-f4f2f970aae7",
            "message": { "text": "!quote cheer100 hello" },
            "cheer": { "bits": 100 },
            "source_broadcaster_user_id": "other",
            "source_broadcaster_user_login": "other_chan",
            "source_broadcaster_user_name": "OtherChan",
        }));
        let event = published(sub).await;

        let mut registry = forge_registry::TriggerRegistry::new();
        crate::triggers::register_twitch_triggers(&mut registry).unwrap();
        let expected =
            forge_types::Variant::String("cc106a89-1814-919d-454c-f4f2f970aae7".to_owned());
        for trigger in [
            crate::vocabulary::trigger_ids::CHAT_MESSAGE,
            crate::vocabulary::trigger_ids::CHAT_COMMAND,
            crate::vocabulary::trigger_ids::CHAT_CHEER_MESSAGE,
            crate::vocabulary::trigger_ids::SHARED_CHAT_MESSAGE_RECEIVED,
        ] {
            let stack = registry.get(trigger).unwrap().build_arg_stack(&event);
            for name in ["message_id", "chat.message_id"] {
                assert_eq!(stack.get(name), Some(&expected), "'{trigger}' '{name}'");
            }
        }
    }

    #[tokio::test]
    async fn a_chat_message_without_a_display_name_carries_the_login_in_its_place() {
        for absent in [serde_json::json!(""), serde_json::json!(null)] {
            let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
            let session = make_session(&bus);
            let sub = bus.subscribe();

            session.publish_chat_message(&serde_json::json!({
                "broadcaster_user_login": "streamer",
                "chatter_user_id": "222",
                "chatter_user_login": "loyalfan",
                "chatter_user_name": absent,
                "message": { "text": "hello there" },
            }));

            let event = published(sub).await;
            assert_eq!(
                event.payload["user"]["display_name"].as_str(),
                Some("loyalfan"),
                "display name {absent}"
            );
        }
    }

    #[tokio::test]
    async fn a_gift_sub_carries_the_total_the_wire_reports_and_zero_when_it_is_silent() {
        for (wire, expected) in [
            (serde_json::json!({ "total": 5 }), 5),
            (serde_json::json!({}), 0),
        ] {
            let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
            let session = make_session(&bus);
            let sub = bus.subscribe();

            let mut event_data = serde_json::json!({
                "user_id": "333",
                "user_login": "generous_viewer",
                "user_name": "GenerousViewer",
                "tier": "1000",
                "is_anonymous": false,
            });
            if let Some(total) = wire.get("total") {
                event_data["total"] = total.clone();
            }
            session.publish_gift_sub_event(&event_data, "meta-gift-001");

            let event = published(sub).await;
            assert_eq!(
                event.payload["gift_total"].as_i64(),
                Some(expected),
                "wire {wire}"
            );
        }
    }

    #[tokio::test]
    async fn raid_received_attaches_event_detail_raid() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "from_broadcaster_user_id": "666",
            "from_broadcaster_user_login": "big_streamer",
            "from_broadcaster_user_name": "BigStreamer",
            "to_broadcaster_user_id": "bcast",
            "viewers": 500u64
        });
        session.publish_raid_event(&event_data, "meta-raid-001");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(ev.kind, "twitch.channel.raid");
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
    async fn ban_event_attaches_a_user_removal_keyed_by_the_banned_user_id() {
        let cases = [
            (
                serde_json::json!({ "user_id": "777", "user_name": "ViewerOne", "is_permanent": true }),
                Some("777"),
                UserRemoval::Ban,
            ),
            (
                serde_json::json!({ "user_id": "777", "user_name": "ViewerOne", "is_permanent": false }),
                Some("777"),
                UserRemoval::Timeout,
            ),
            (
                serde_json::json!({ "user_id": "", "user_name": "ViewerOne", "is_permanent": true }),
                None,
                UserRemoval::Ban,
            ),
            (
                serde_json::json!({ "user_name": "ViewerOne", "is_permanent": false }),
                None,
                UserRemoval::Timeout,
            ),
        ];

        for (event_data, user_id, removal) in cases {
            let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
            let session = make_session(&bus);
            let mut sub = bus.subscribe();

            session.publish_ban_event(&event_data, "meta-ban-003");

            let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
                .await
                .unwrap()
                .unwrap();
            let attached: ChatModerationPayload =
                serde_json::from_value(ev.payload[ChatModerationPayload::KEY].clone()).unwrap();
            assert_eq!(
                attached.action,
                ChatModerationAction::RemoveUser {
                    user_id: user_id.map(str::to_string),
                    user_name: "ViewerOne".to_string(),
                    removal,
                },
                "{event_data}"
            );
        }
    }

    #[tokio::test]
    async fn automod_hold_terms_found_keeps_only_entries_carrying_a_string_term_id() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "user_login": "viewer_one",
            "message_id": "hold-blocked-2",
            "message": {"text": "a banned phrase"},
            "reason": "blocked_term",
            "blocked_term": {
                "terms_found": [
                    {"term_id": "term-1"},
                    {"boundary": {"start_pos": 0, "end_pos": 3}},
                    {"term_id": 42},
                    "term-loose",
                    {"term_id": "term-2"}
                ]
            }
        });
        session.publish_automod_hold_event(&event_data, "meta-automod-malformed");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            ev.payload["blocked_term"]["terms_found"],
            serde_json::json!(["term-1", "term-2"])
        );
    }

    #[tokio::test]
    async fn automod_message_update_with_blocked_term_reason_nulls_the_category_and_level() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "user_login": "viewer_one",
            "message_id": "msg-blocked-1",
            "message": {"text": "a banned phrase"},
            "status": "Denied",
            "reason": "blocked_term",
            "blocked_term": {
                "terms_found": [{"term_id": "term-9"}]
            }
        });
        session.publish_automod_message_update_event(&event_data, "meta-automod-msg-blocked");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(ev.payload["automod"]["category"].is_null());
        assert!(ev.payload["automod"]["level"].is_null());
        assert_eq!(ev.payload["reason"].as_str(), Some("blocked_term"));
        assert_eq!(
            ev.payload["blocked_term"]["terms_found"][0].as_str(),
            Some("term-9")
        );
    }

    #[tokio::test]
    async fn automod_hold_message_text_reads_object_form_and_ignores_plain_string() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        for (message, expected) in [
            (serde_json::json!({"text": "held words"}), "held words"),
            (serde_json::json!("held words"), ""),
        ] {
            let event_data = serde_json::json!({
                "user_login": "viewer_one",
                "message_id": "hold-1",
                "message": message,
                "automod": {"category": "harassment", "level": 1}
            });
            session.publish_automod_hold_event(&event_data, "meta");

            let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
                .await
                .unwrap()
                .unwrap();

            assert_eq!(ev.payload["message_text"].as_str(), Some(expected));
        }
    }

    #[tokio::test]
    async fn publish_guest_star_guest_update_event_nulls_absent_guest_identity() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let mut sub = bus.subscribe();

        let event_data = serde_json::json!({
            "session_id": "sess-7",
            "slot_id": "3",
            "state": "removed",
        });
        session.publish_guest_star_guest_update_event(&event_data, "meta-gs-slot-only");

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            ev.payload["guest"]["id"].is_null(),
            "omitted guest identity must be null, not empty string"
        );
        assert!(ev.payload["guest"]["login"].is_null());
        assert!(ev.payload["guest"]["display_name"].is_null());
    }

    const SESSION_WELCOME_FRAME: &str = r#"{"metadata":{"message_type":"session_welcome","message_id":"abc"},"payload":{"session":{"id":"sess-123","reconnect_url":null}}}"#;
    const EVENTSUB_SUBSCRIPTIONS_PATH: &str = "/helix/eventsub/subscriptions";

    fn session_over_creds(
        bus: &Arc<EventChannel>,
        repo: Arc<dyn forge_storage::CredentialsRepo>,
    ) -> ChatSession {
        let (session, _state_rx, _shutdown_tx) = session_with(
            bus,
            repo,
            crate::sub_actions::test_support::unreachable_twitch_endpoints(),
        );
        session
    }

    fn session_with(
        bus: &Arc<EventChannel>,
        repo: Arc<dyn forge_storage::CredentialsRepo>,
        endpoints: PlatformEndpoints,
    ) -> (
        ChatSession,
        watch::Receiver<ChatConnectionState>,
        oneshot::Sender<()>,
    ) {
        let manager = Arc::new(TwitchCredentialsManager::new(
            crate::sub_actions::test_support::unreachable_oauth_client("client"),
            repo,
        ));
        let publisher: Arc<dyn EventPublisher> = bus.clone();
        ChatSession::new(
            manager,
            helix_over(&endpoints, Arc::clone(&publisher)),
            session_config(endpoints),
            publisher,
            crate::subscriptions::SubscriptionTracker::default(),
            TwitchLifecycle::new(),
        )
    }

    fn helix_over(endpoints: &PlatformEndpoints, bus: Arc<dyn EventPublisher>) -> Gateway {
        crate::helix_gateway::helix_gateway(
            endpoints,
            Arc::new(forge_net::TokenBucketRateLimiter::new(
                crate::config::HELIX_BUDGET_CAPACITY,
                crate::config::HELIX_BUDGET_WINDOW,
            )),
            bus,
        )
        .unwrap()
    }

    struct FailingCreds;

    #[async_trait::async_trait]
    impl forge_storage::CredentialsRepo for FailingCreds {
        async fn store(
            &self,
            _: &forge_storage::CredentialId,
            _: &str,
        ) -> Result<(), forge_storage::StorageError> {
            Ok(())
        }

        async fn load(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<Option<String>, forge_storage::StorageError> {
            Err(forge_storage::StorageError::Connection {
                reason: "backend offline".to_owned(),
            })
        }

        async fn delete(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<bool, forge_storage::StorageError> {
            Ok(false)
        }

        async fn list_ids(
            &self,
        ) -> Result<Vec<forge_storage::CredentialId>, forge_storage::StorageError> {
            Ok(Vec::new())
        }

        async fn last_refresh(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<Option<time::OffsetDateTime>, forge_storage::StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<(), forge_storage::StorageError> {
            Ok(())
        }
    }

    const TOKEN_BODY_SENTINEL: &str = "SESSION_TOKEN_BODY_SENTINEL_m2";

    struct NearExpiryCreds;

    #[async_trait::async_trait]
    impl forge_storage::CredentialsRepo for NearExpiryCreds {
        async fn store(
            &self,
            _: &forge_storage::CredentialId,
            _: &str,
        ) -> Result<(), forge_storage::StorageError> {
            Ok(())
        }

        async fn load(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<Option<String>, forge_storage::StorageError> {
            let expires_at_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64 + 60)
                .unwrap_or(0);
            Ok(Some(
                serde_json::json!({
                    "access_token": "stale_access",
                    "refresh_token": "stale_refresh",
                    "user_id": "user_1",
                    "login": "streamer",
                    "expires_at_unix": expires_at_unix,
                })
                .to_string(),
            ))
        }

        async fn delete(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<bool, forge_storage::StorageError> {
            Ok(false)
        }

        async fn list_ids(
            &self,
        ) -> Result<Vec<forge_storage::CredentialId>, forge_storage::StorageError> {
            Ok(Vec::new())
        }

        async fn last_refresh(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<Option<time::OffsetDateTime>, forge_storage::StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<(), forge_storage::StorageError> {
            Ok(())
        }
    }

    fn session_welcome_over_failing_token_endpoint()
    -> (FrameAction, Vec<crate::log_capture::CapturedLine>) {
        crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let server = wiremock::MockServer::builder().start().await;
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path("/token"))
                .respond_with(wiremock::ResponseTemplate::new(503).set_body_json(
                    serde_json::json!({
                        "message": TOKEN_BODY_SENTINEL,
                    }),
                ))
                .mount(&server)
                .await;
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
                .respond_with(wiremock::ResponseTemplate::new(500))
                .expect(0)
                .mount(&server)
                .await;

            let manager = Arc::new(TwitchCredentialsManager::new(
                crate::sub_actions::test_support::oauth_client_at(&server.uri(), "client"),
                Arc::new(NearExpiryCreds),
            ));
            let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
            let publisher: Arc<dyn EventPublisher> = bus.clone();
            let endpoints = crate::sub_actions::test_support::endpoints_with(&[(
                EndpointSurface::TwitchApi,
                &server.uri(),
            )]);
            let (mut session, _state_rx, _shutdown_tx) = ChatSession::new(
                manager,
                helix_over(&endpoints, Arc::clone(&publisher)),
                session_config(endpoints),
                publisher,
                crate::subscriptions::SubscriptionTracker::default(),
                TwitchLifecycle::new(),
            );

            let mut session_id = None;
            session
                .handle_frame(SESSION_WELCOME_FRAME, &mut session_id)
                .await
        })
    }

    #[test]
    fn session_welcome_token_failure_never_logs_the_endpoint_response_body() {
        let (action, lines) = session_welcome_over_failing_token_endpoint();
        assert!(
            matches!(action, FrameAction::Disconnect),
            "a non-reauth token error must disconnect for backoff retry"
        );

        let forge_lines = crate::log_capture::forge_lines(&lines);
        assert!(
            !forge_lines.is_empty(),
            "the session_welcome path must have logged something to inspect"
        );
        for line in forge_lines {
            assert!(
                !line.mentions(TOKEN_BODY_SENTINEL),
                "token-endpoint response body reached a {} line: {:?}",
                line.level,
                line.fields
            );
        }
    }

    #[test]
    fn session_welcome_token_failure_warns_with_the_upstream_status() {
        let (_, lines) = session_welcome_over_failing_token_endpoint();

        let session_warns: Vec<_> = crate::log_capture::forge_lines(&lines)
            .into_iter()
            .filter(|line| {
                line.level == tracing::Level::WARN && line.target.ends_with("chat::session")
            })
            .collect();

        assert!(
            session_warns
                .iter()
                .any(|line| line.field("status") == Some("503")),
            "the disconnect WARN must report the upstream status so a 5xx is distinguishable \
             from a network drop; got {:?}",
            session_warns.iter().map(|l| &l.fields).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn session_welcome_routes_to_reauth_when_token_refresh_reports_reauth_required() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let mut session = session_over_creds(&bus, Arc::new(MockCreds::empty()));
        let mut sub = bus.subscribe();

        let mut session_id = None;
        let action = session
            .handle_frame(SESSION_WELCOME_FRAME, &mut session_id)
            .await;

        assert!(
            matches!(action, FrameAction::ReauthRequired),
            "missing credential at session_welcome must route to reauth, not disconnect"
        );

        let ev = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .expect("a reauth event must be published")
            .expect("recv error");
        assert_eq!(ev.kind, "platform.reauth_required");
        assert_eq!(ev.payload["platform_id"].as_str(), Some("twitch"));
    }

    #[tokio::test]
    async fn session_welcome_disconnects_when_token_fetch_fails_without_reauth() {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let mut session = session_over_creds(&bus, Arc::new(FailingCreds));

        let mut session_id = None;
        let action = session
            .handle_frame(SESSION_WELCOME_FRAME, &mut session_id)
            .await;

        assert!(
            matches!(action, FrameAction::Disconnect),
            "a non-reauth token error must disconnect for backoff retry, not route to reauth"
        );
    }

    #[tokio::test]
    async fn session_welcome_creates_eventsub_subscriptions_on_the_twitch_api_override() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
            .respond_with(
                wiremock::ResponseTemplate::new(202).set_body_json(serde_json::json!({
                    "data": [{ "id": "sub-1", "type": "generic", "condition": {} }]
                })),
            )
            .mount(&server)
            .await;
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let endpoints = crate::sub_actions::test_support::endpoints_with(&[
            (EndpointSurface::TwitchApi, &server.uri()),
            (EndpointSurface::TwitchEventSubSocket, "ws://127.0.0.1:1"),
        ]);
        let (mut session, _state_rx, _shutdown_tx) =
            session_with(&bus, Arc::new(MockCreds::with_identity()), endpoints);

        let mut session_id = None;
        if let FrameAction::Subscribe(pass) = session
            .handle_frame(SESSION_WELCOME_FRAME, &mut session_id)
            .await
        {
            pass.await;
        }

        let requests = server.received_requests().await.unwrap();
        assert!(
            requests.iter().any(|request| {
                request.url.path() == EVENTSUB_SUBSCRIPTIONS_PATH
                    && serde_json::from_slice::<serde_json::Value>(&request.body).is_ok_and(
                        |body| body["transport"]["session_id"].as_str() == Some("sess-123"),
                    )
            }),
            "subscription creation for the welcomed session must reach the TwitchApi override; \
             got {} request(s)",
            requests.len()
        );
    }

    const FAKE_WAIT: Duration = Duration::from_secs(30);

    async fn within_real_time<T>(
        bound: Duration,
        work: impl Future<Output = T>,
    ) -> Result<T, Duration> {
        let (expiry_tx, expiry) = oneshot::channel::<()>();
        std::thread::spawn(move || {
            std::thread::sleep(bound);
            drop(expiry_tx);
        });
        tokio::select! {
            done = work => Ok(done),
            _ = expiry => Err(bound),
        }
    }
    const CHAT_MESSAGE_KIND: &str = "twitch.channel.chat.message";

    enum PeerCommand {
        Text(String),
        Ping,
        Close,
    }

    struct Peer {
        commands: mpsc::UnboundedSender<PeerCommand>,
        closed: oneshot::Receiver<()>,
        pongs: mpsc::UnboundedReceiver<()>,
    }

    impl Peer {
        fn send(&self, frame: String) {
            self.commands
                .send(PeerCommand::Text(frame))
                .expect("the fake socket must outlive the frames the test sends");
        }

        fn close(&self) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
            let _ = self.commands.send(PeerCommand::Close);
        }

        async fn wait_until_forge_read_everything(&mut self) {
            self.commands
                .send(PeerCommand::Ping)
                .expect("the fake socket must outlive the barrier");
            let answered = within_real_time(FAKE_WAIT, self.pongs.recv()).await;
            assert!(
                answered.is_ok_and(|pong| pong.is_some()),
                "forge must keep reading its socket"
            );
        }

        fn closed_by_forge(&mut self) -> bool {
            self.closed.try_recv().is_ok()
        }

        async fn wait_closed_by_forge(&mut self) -> bool {
            within_real_time(FAKE_WAIT, &mut self.closed)
                .await
                .is_ok_and(|closed| closed.is_ok())
        }
    }

    struct FakeEventSub {
        url: String,
        peers: mpsc::UnboundedReceiver<Peer>,
    }

    impl FakeEventSub {
        async fn bind() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let (peer_tx, peers) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
                        continue;
                    };
                    let (commands, command_rx) = mpsc::unbounded_channel();
                    let (closed_tx, closed) = oneshot::channel();
                    let (pong_tx, pongs) = mpsc::unbounded_channel();
                    let peer = Peer {
                        commands,
                        closed,
                        pongs,
                    };
                    if peer_tx.send(peer).is_err() {
                        return;
                    }
                    tokio::spawn(serve_peer(socket, command_rx, closed_tx, pong_tx));
                }
            });
            Self { url, peers }
        }

        async fn accept(&mut self) -> Peer {
            within_real_time(FAKE_WAIT, self.peers.recv())
                .await
                .expect("forge must dial this socket")
                .expect("the accept loop must outlive the test")
        }

        fn no_dial_pending(&mut self) -> bool {
            matches!(self.peers.try_recv(), Err(mpsc::error::TryRecvError::Empty))
        }
    }

    async fn serve_peer(
        socket: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        mut commands: mpsc::UnboundedReceiver<PeerCommand>,
        closed: oneshot::Sender<()>,
        pongs: mpsc::UnboundedSender<()>,
    ) {
        use futures_util::SinkExt;
        let mut closed = Some(closed);
        let (mut sink, mut source) = socket.split();
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(PeerCommand::Text(frame)) => {
                        if sink.send(Message::Text(frame.into())).await.is_err() {
                            return;
                        }
                    }
                    Some(PeerCommand::Ping) => {
                        if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                            return;
                        }
                    }
                    Some(PeerCommand::Close) | None => {
                        #[expect(clippy::let_underscore_must_use, reason = "a failed close does not affect the scenario")]
                        let _ = sink.close().await;
                        return;
                    }
                },
                frame = source.next() => match frame {
                    Some(Ok(Message::Close(_))) | None => {
                        if let Some(closed) = closed.take() {
                            #[expect(clippy::let_underscore_must_use, reason = "the receiver may already be gone")]
                            let _ = closed.send(());
                        }
                        return;
                    }
                    Some(Err(_)) => return,
                    Some(Ok(Message::Pong(_))) => {
                        #[expect(clippy::let_underscore_must_use, reason = "the receiver may already be gone")]
                        let _ = pongs.send(());
                    }
                    Some(Ok(_)) => {}
                },
            }
        }
    }

    fn unreachable_ws_url() -> String {
        "ws://127.0.0.1:0".to_owned()
    }

    struct RunningSession {
        bus: Arc<EventChannel>,
        state: watch::Receiver<ChatConnectionState>,
        task: tokio::task::JoinHandle<()>,
        _shutdown: oneshot::Sender<()>,
    }

    impl Drop for RunningSession {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    fn spawn_session(socket_url: &str, api_url: &str) -> RunningSession {
        spawn_session_timed(socket_url, api_url, ChatTiming::default())
    }

    fn spawn_session_timed(socket_url: &str, api_url: &str, timing: ChatTiming) -> RunningSession {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let endpoints = crate::sub_actions::test_support::endpoints_with(&[
            (EndpointSurface::TwitchApi, api_url),
            (EndpointSurface::TwitchEventSubSocket, socket_url),
        ]);
        let (session, state, shutdown) =
            session_with(&bus, Arc::new(MockCreds::with_identity()), endpoints);
        RunningSession {
            bus,
            state,
            task: tokio::spawn(session.with_timing(timing).run()),
            _shutdown: shutdown,
        }
    }

    fn welcome_frame(session_id: &str) -> String {
        serde_json::json!({
            "metadata": {
                "message_type": "session_welcome",
                "message_id": format!("welcome-{session_id}"),
            },
            "payload": { "session": { "id": session_id, "reconnect_url": null } },
        })
        .to_string()
    }

    fn reconnect_frame(reconnect_url: &str) -> String {
        serde_json::json!({
            "metadata": { "message_type": "session_reconnect", "message_id": "reconnect" },
            "payload": { "session": { "id": "sess-1", "reconnect_url": reconnect_url } },
        })
        .to_string()
    }

    fn reconnect_frame_without_url() -> String {
        serde_json::json!({
            "metadata": { "message_type": "session_reconnect", "message_id": "reconnect-no-url" },
            "payload": { "session": { "id": "sess-1", "reconnect_url": null } },
        })
        .to_string()
    }

    fn chat_frame(message_id: &str, text: &str) -> String {
        serde_json::json!({
            "metadata": {
                "message_type": "notification",
                "message_id": message_id,
                "subscription_type": "channel.chat.message",
            },
            "payload": { "event": {
                "broadcaster_user_login": "streamer",
                "chatter_user_id": "42",
                "chatter_user_login": "viewer",
                "message": { "text": text },
            }},
        })
        .to_string()
    }

    async fn next_event(events: &mut EventSubscription, kind: &str) -> Event {
        let found = within_real_time(FAKE_WAIT, async {
            loop {
                let event = events.recv().await.expect("the bus must stay open");
                if event.kind == kind {
                    return event;
                }
            }
        })
        .await;

        assert!(
            found.is_ok(),
            "no {kind} event arrived within {FAKE_WAIT:?}"
        );
        found.expect("asserted just above")
    }

    async fn next_chat_text(events: &mut EventSubscription) -> String {
        next_event(events, CHAT_MESSAGE_KIND).await.payload["message"]
            .as_str()
            .expect("a chat event must carry its message text")
            .to_owned()
    }

    async fn kinds_until_chat(events: &mut EventSubscription, text: &str) -> Vec<String> {
        let mut kinds = Vec::new();
        let arrived = within_real_time(FAKE_WAIT, async {
            loop {
                let event = events.recv().await.expect("the bus must stay open");
                let awaited = event.kind == CHAT_MESSAGE_KIND
                    && event.payload["message"].as_str() == Some(text);
                kinds.push(event.kind);
                if awaited {
                    return;
                }
            }
        })
        .await;

        assert!(
            arrived.is_ok(),
            "the chat message {text:?} never arrived; saw {kinds:?}"
        );
        kinds
    }

    async fn chat_texts_until(events: &mut EventSubscription, last: &str) -> Vec<String> {
        let mut texts = Vec::new();
        let arrived = within_real_time(FAKE_WAIT, async {
            loop {
                let event = events.recv().await.expect("the bus must stay open");
                if event.kind != CHAT_MESSAGE_KIND {
                    continue;
                }
                let text = event.payload["message"]
                    .as_str()
                    .expect("a chat event must carry its message text")
                    .to_owned();
                let awaited = text == last;
                texts.push(text);
                if awaited {
                    return;
                }
            }
        })
        .await;

        assert!(
            arrived.is_ok(),
            "the chat message {last:?} never arrived; saw {texts:?}"
        );
        texts
    }

    async fn next_retry_attempt(state: &mut watch::Receiver<ChatConnectionState>) -> u8 {
        let reported = within_real_time(FAKE_WAIT, async {
            loop {
                state
                    .changed()
                    .await
                    .expect("the session task must outlive the state it reports");
                if let ChatConnectionState::Reconnecting { attempt } = *state.borrow_and_update() {
                    return attempt;
                }
            }
        })
        .await;

        reported.expect("the session must report a retry after its socket drops")
    }

    async fn connection_states_until(
        events: &mut EventSubscription,
        last: ConnectionState,
    ) -> Vec<ConnectionState> {
        let mut states = Vec::new();
        let arrived = within_real_time(FAKE_WAIT, async {
            loop {
                let event = events.recv().await.expect("the bus must stay open");
                if event.kind != PlatformConnectionChanged::KIND {
                    continue;
                }
                let state: ConnectionState = serde_json::from_value(event.payload["state"].clone())
                    .expect("a connection-state event must carry a known state");
                states.push(state);
                if state == last {
                    return;
                }
            }
        })
        .await;

        assert!(
            arrived.is_ok(),
            "the session never reported {last:?}; saw {states:?}"
        );
        states
    }

    fn subscription_accepted() -> wiremock::ResponseTemplate {
        wiremock::ResponseTemplate::new(reqwest::StatusCode::ACCEPTED.as_u16()).set_body_json(
            serde_json::json!({
                "data": [{ "id": "sub-1", "type": "generic", "condition": {} }]
            }),
        )
    }

    async fn eventsub_api() -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
            .respond_with(subscription_accepted())
            .mount(&server)
            .await;
        server
    }

    async fn eventsub_api_accepting_only(live_kinds: &[&str]) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        for kind in live_kinds {
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
                .and(wiremock::matchers::body_string_contains(format!(
                    r#""type":"{kind}""#
                )))
                .respond_with(subscription_accepted())
                .with_priority(1)
                .mount(&server)
                .await;
        }
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
            .respond_with(wiremock::ResponseTemplate::new(
                reqwest::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ))
            .mount(&server)
            .await;
        server
    }

    async fn subscription_posts(server: &wiremock::MockServer) -> usize {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == EVENTSUB_SUBSCRIPTIONS_PATH)
            .count()
    }

    struct Live {
        api: wiremock::MockServer,
        base: FakeEventSub,
        session: RunningSession,
        events: EventSubscription,
        predecessor: Peer,
    }

    async fn live_session() -> Live {
        let api = eventsub_api().await;
        let mut base = FakeEventSub::bind().await;
        let session = spawn_session(&base.url, &api.uri());
        let mut events = session.bus.subscribe();
        let predecessor = base.accept().await;

        predecessor.send(welcome_frame("sess-1"));
        predecessor.send(chat_frame("first", "before any reconnect"));
        assert_eq!(
            next_chat_text(&mut events).await,
            "before any reconnect",
            "the first session must be delivering chat before a test drives a reconnect"
        );

        Live {
            api,
            base,
            session,
            events,
            predecessor,
        }
    }

    async fn overlapping_session() -> (Live, FakeEventSub, Peer) {
        let live = live_session().await;
        let mut successor_socket = FakeEventSub::bind().await;

        live.predecessor
            .send(reconnect_frame(&successor_socket.url));
        let successor = successor_socket.accept().await;

        (live, successor_socket, successor)
    }

    #[tokio::test]
    async fn a_successor_session_runs_no_second_subscription_pass() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;
        let first_pass = subscription_posts(&live.api).await;
        assert!(first_pass > 0, "the first session must have subscribed");

        successor.send(welcome_frame("sess-2"));
        successor.send(chat_frame("after", "on the successor"));
        assert_eq!(next_chat_text(&mut live.events).await, "on the successor");

        assert_eq!(
            subscription_posts(&live.api).await,
            first_pass,
            "the successor inherits the predecessor's subscriptions, so a second pass is only \
             one HTTP 409 per topic"
        );
    }

    #[tokio::test]
    async fn the_predecessor_keeps_delivering_until_the_successor_welcomes() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;

        live.predecessor
            .send(chat_frame("overlap", "sent during the overlap"));
        assert_eq!(
            next_chat_text(&mut live.events).await,
            "sent during the overlap",
            "a notification the predecessor sends before the successor welcomes must publish"
        );
        assert!(
            !live.predecessor.closed_by_forge(),
            "the predecessor must stay open while the successor has not welcomed"
        );

        successor.send(welcome_frame("sess-2"));

        assert!(
            live.predecessor.wait_closed_by_forge().await,
            "the predecessor must be closed once the successor welcomes"
        );
    }

    #[tokio::test]
    async fn a_notification_repeated_across_the_switchover_is_published_once() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;
        live.predecessor.send(chat_frame("dup", "delivered twice"));
        assert_eq!(next_chat_text(&mut live.events).await, "delivered twice");

        successor.send(welcome_frame("sess-2"));
        successor.send(chat_frame("dup", "delivered twice"));
        successor.send(chat_frame("next", "the one after"));

        assert_eq!(
            next_chat_text(&mut live.events).await,
            "the one after",
            "the resend the overlap duplicates must not publish a second chat event"
        );
    }

    enum SuccessorFailure {
        DialRefused,
        ClosedBeforeWelcome,
    }

    #[tokio::test]
    async fn a_failed_successor_falls_back_to_the_predecessor_and_then_to_a_fresh_session() {
        for failure in [
            SuccessorFailure::DialRefused,
            SuccessorFailure::ClosedBeforeWelcome,
        ] {
            let mut live = live_session().await;
            match failure {
                SuccessorFailure::DialRefused => {
                    live.predecessor
                        .send(reconnect_frame(&unreachable_ws_url()));
                }
                SuccessorFailure::ClosedBeforeWelcome => {
                    let mut successor_socket = FakeEventSub::bind().await;
                    live.predecessor
                        .send(reconnect_frame(&successor_socket.url));
                    successor_socket.accept().await.close();
                }
            }

            live.predecessor.send(chat_frame(
                "after-failure",
                "the predecessor still delivers",
            ));
            assert_eq!(
                next_chat_text(&mut live.events).await,
                "the predecessor still delivers",
                "a successor that never welcomes must not end the session"
            );

            live.predecessor.close();

            let fresh = live.base.accept().await;
            fresh.send(welcome_frame("sess-2"));
            fresh.send(chat_frame("fresh", "the replacement session"));
            assert_eq!(
                next_chat_text(&mut live.events).await,
                "the replacement session",
                "losing the predecessor after a failed successor must return to the ordinary \
                 disconnect path"
            );
        }
    }

    #[tokio::test]
    async fn the_session_after_a_reconnect_redials_the_base_url_and_resubscribes() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;
        successor.send(welcome_frame("sess-2"));
        successor.send(chat_frame("after", "on the successor"));
        assert_eq!(next_chat_text(&mut live.events).await, "on the successor");
        let posts_before = subscription_posts(&live.api).await;

        successor.close();

        let fresh = live.base.accept().await;
        fresh.send(welcome_frame("sess-3"));
        fresh.send(chat_frame("fresh", "the replacement session"));
        assert_eq!(
            next_chat_text(&mut live.events).await,
            "the replacement session"
        );
        assert!(
            subscription_posts(&live.api).await > posts_before,
            "a session Twitch did not hand the subscriptions to must run a full pass"
        );
    }

    #[test]
    fn a_failed_successor_dial_never_logs_the_reconnect_url() {
        const RECONNECT_TOKEN: &str = "RECONNECT_TOKEN_SENTINEL_q4";
        let (_, lines) = crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let mut live = live_session().await;
            let dead = unreachable_ws_url();
            live.predecessor.send(reconnect_frame(&format!(
                "{dead}/?reconnect_token={RECONNECT_TOKEN}"
            )));
            live.predecessor.close();
            live.base.accept().await;
        });

        let forge_lines = crate::log_capture::forge_lines(&lines);
        assert!(
            forge_lines
                .iter()
                .any(|line| line.message() == "successor eventsub connection failed"),
            "the fixture must have driven the failing dial it exists to inspect"
        );
        for line in forge_lines {
            assert!(
                !line.mentions(RECONNECT_TOKEN),
                "the reconnect URL reached a {} line: {:?}",
                line.level,
                line.fields
            );
        }
    }

    #[tokio::test]
    async fn a_repeated_reconnect_while_the_successor_dials_is_ignored() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;
        let mut second_socket = FakeEventSub::bind().await;

        live.predecessor.send(reconnect_frame(&second_socket.url));
        successor.send(welcome_frame("sess-2"));
        successor.send(chat_frame("after", "on the first successor"));

        assert_eq!(
            next_chat_text(&mut live.events).await,
            "on the first successor",
            "the dial already in flight must stay the one the session adopts"
        );
        assert!(
            second_socket.no_dial_pending(),
            "a reconnect arriving while a successor dials must not open a second connection"
        );
    }

    #[tokio::test]
    async fn a_server_initiated_reconnect_never_leaves_the_connected_state() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;
        assert_eq!(
            *live.session.state.borrow_and_update(),
            ChatConnectionState::Connected
        );

        successor.send(welcome_frame("sess-2"));
        successor.send(chat_frame("after", "on the successor"));
        let kinds = kinds_until_chat(&mut live.events, "on the successor").await;

        assert!(
            !kinds
                .iter()
                .any(|kind| kind == PlatformConnectionChanged::KIND),
            "moving to the successor is invisible to a connected viewer, so it must publish no \
             connection-state event; saw {kinds:?}"
        );
        assert!(
            !live.session.state.has_changed().unwrap(),
            "the switchover must not blip the connection state"
        );
    }

    #[tokio::test]
    async fn a_session_that_subscribed_restarts_the_retry_count_for_the_next_drop() {
        let mut live = live_session().await;

        live.predecessor.close();
        assert_eq!(
            next_retry_attempt(&mut live.session.state).await,
            1,
            "the first dial owns `Connecting`, so the drop after it retries as attempt one"
        );

        let second = live.base.accept().await;
        second.send(welcome_frame("sess-2"));
        second.send(chat_frame("second", "on the second session"));
        assert_eq!(
            next_chat_text(&mut live.events).await,
            "on the second session"
        );

        second.close();
        assert_eq!(
            next_retry_attempt(&mut live.session.state).await,
            1,
            "a session that welcomed and subscribed proves the endpoint healthy, so the drop \
             after it must retry from the same attempt as the very first drop"
        );
    }

    #[tokio::test]
    async fn a_welcome_whose_every_topic_fails_never_reports_connected() {
        let api = eventsub_api_accepting_only(&[]).await;
        let mut base = FakeEventSub::bind().await;
        let session = spawn_session(&base.url, &api.uri());
        let mut events = session.bus.subscribe();
        let welcomed = base.accept().await;

        welcomed.send(welcome_frame("sess-1"));

        let states = connection_states_until(&mut events, ConnectionState::Reconnecting).await;
        assert!(
            !states.contains(&ConnectionState::Connected),
            "a socket holding no subscription receives nothing but keepalives, so claiming \
             Connected would light the health dot on a dead session; saw {states:?}"
        );
    }

    #[tokio::test]
    async fn consecutive_welcomes_whose_topics_all_fail_keep_growing_the_retry_count() {
        let api = eventsub_api_accepting_only(&[]).await;
        let mut base = FakeEventSub::bind().await;
        let mut session = spawn_session(&base.url, &api.uri());

        let first = base.accept().await;
        first.send(welcome_frame("sess-1"));
        assert_eq!(next_retry_attempt(&mut session.state).await, 1);

        let second = base.accept().await;
        second.send(welcome_frame("sess-2"));
        assert_eq!(
            next_retry_attempt(&mut session.state).await,
            2,
            "a welcome that subscribed to nothing is no proof of a healthy endpoint, so the \
             retry count must keep climbing instead of restarting"
        );
    }

    #[tokio::test]
    async fn a_welcome_leaving_one_topic_live_among_failures_still_connects() {
        let api = eventsub_api_accepting_only(&["channel.chat.message"]).await;
        let mut base = FakeEventSub::bind().await;
        let session = spawn_session(&base.url, &api.uri());
        let mut events = session.bus.subscribe();
        let welcomed = base.accept().await;

        welcomed.send(welcome_frame("sess-1"));

        let states = connection_states_until(&mut events, ConnectionState::Connected).await;
        assert!(
            !states.contains(&ConnectionState::Reconnecting),
            "one live topic is a healthy session, so it must connect without being retired and \
             redialled first; saw {states:?}"
        );
    }

    const BUFFERED_OVERLAP_FRAMES: usize = 8;

    #[tokio::test]
    async fn frames_buffered_on_the_predecessor_publish_exactly_once_across_the_switchover() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;
        let buffered: Vec<String> = (0..BUFFERED_OVERLAP_FRAMES)
            .map(|n| format!("buffered {n}"))
            .collect();

        for (n, text) in buffered.iter().enumerate() {
            live.predecessor
                .send(chat_frame(&format!("buffered-{n}"), text));
        }
        successor.send(welcome_frame("sess-2"));
        successor.send(chat_frame("buffered-0", &buffered[0]));
        successor.send(chat_frame("after", "on the successor"));

        let expected: Vec<String> = buffered
            .iter()
            .cloned()
            .chain(std::iter::once("on the successor".to_owned()))
            .collect();
        assert_eq!(
            chat_texts_until(&mut live.events, "on the successor").await,
            expected,
            "frames the predecessor delivered before the successor welcomed must survive the \
             switchover, and Twitch's resend of one of them on the successor must not repeat it"
        );
    }

    #[tokio::test]
    async fn a_disconnect_buffered_on_the_predecessor_still_ends_the_session() {
        let (mut live, _successor_socket, successor) = overlapping_session().await;

        let ahead: Vec<String> = (0..BUFFERED_OVERLAP_FRAMES)
            .map(|n| format!("ahead {n}"))
            .collect();

        for (n, text) in ahead.iter().enumerate() {
            live.predecessor
                .send(chat_frame(&format!("ahead-{n}"), text));
        }
        live.predecessor.send(reconnect_frame_without_url());
        successor.send(welcome_frame("sess-2"));

        let fresh = live.base.accept().await;
        fresh.send(welcome_frame("sess-3"));
        fresh.send(chat_frame("fresh", "the replacement session"));

        let expected: Vec<String> = ahead
            .iter()
            .cloned()
            .chain(std::iter::once("the replacement session".to_owned()))
            .collect();
        assert_eq!(
            chat_texts_until(&mut live.events, "the replacement session").await,
            expected,
            "a disconnect the predecessor buffered must end the session rather than retire with \
             the socket that carried it, and the frames queued ahead of it must publish first"
        );
    }

    struct HeldHttpEndpoint {
        url: String,
        requests: mpsc::UnboundedReceiver<()>,
        gate: watch::Sender<bool>,
    }

    impl HeldHttpEndpoint {
        async fn bind() -> Self {
            Self::bind_answering(
                reqwest::StatusCode::ACCEPTED,
                serde_json::json!({
                    "data": [{ "id": "sub-1", "type": "generic", "condition": {} }]
                })
                .to_string(),
            )
            .await
        }

        async fn bind_answering(status: reqwest::StatusCode, body: String) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (gate, released) = watch::channel(false);
            let (request_tx, requests) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(answer_once_released(
                        stream,
                        status,
                        body.clone(),
                        released.clone(),
                        request_tx.clone(),
                    ));
                }
            });
            Self {
                url,
                requests,
                gate,
            }
        }

        fn ws_url(&self) -> String {
            self.url.replacen("http://", "ws://", 1)
        }

        async fn wait_for_a_request(&mut self) {
            let arrived = within_real_time(FAKE_WAIT, self.requests.recv()).await;
            assert!(
                arrived.is_ok_and(|request| request.is_some()),
                "forge must send a request to this endpoint"
            );
        }

        fn release(&self) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
            let _ = self.gate.send(true);
        }
    }

    async fn answer_once_released(
        mut stream: tokio::net::TcpStream,
        status: reqwest::StatusCode,
        body: String,
        mut released: watch::Receiver<bool>,
        requests: mpsc::UnboundedSender<()>,
    ) {
        use tokio::io::AsyncWriteExt;
        let response = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            status.as_u16(),
            status.canonical_reason().unwrap_or_default(),
            body.len()
        );
        let mut buffered = Vec::new();
        while let Some(request_len) = read_one_request(&mut stream, &mut buffered).await {
            buffered.drain(..request_len);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver may already be gone"
            )]
            let _ = requests.send(());
            if released.wait_for(|open| *open).await.is_err()
                || stream.write_all(response.as_bytes()).await.is_err()
            {
                return;
            }
        }
    }

    async fn read_one_request(
        stream: &mut tokio::net::TcpStream,
        buffered: &mut Vec<u8>,
    ) -> Option<usize> {
        use tokio::io::AsyncReadExt;
        const HEAD_END: &[u8] = b"\r\n\r\n";
        loop {
            if let Some(head_len) = buffered
                .windows(HEAD_END.len())
                .position(|window| window == HEAD_END)
            {
                let head = String::from_utf8_lossy(&buffered[..head_len]).to_ascii_lowercase();
                let body_len = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let request_len = head_len + HEAD_END.len() + body_len;
                if buffered.len() >= request_len {
                    return Some(request_len);
                }
            }
            let mut chunk = [0u8; 4096];
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return None,
                Ok(read) => buffered.extend_from_slice(&chunk[..read]),
            }
        }
    }

    #[derive(Default)]
    struct RecordingBus {
        events: std::sync::Mutex<Vec<Event>>,
        published: tokio::sync::Notify,
    }

    impl EventPublisher for RecordingBus {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
            self.published.notify_one();
        }
    }

    impl RecordingBus {
        fn chat_texts(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.kind == CHAT_MESSAGE_KIND)
                .filter_map(|event| event.payload["message"].as_str().map(str::to_owned))
                .collect()
        }

        fn kinds_published(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .map(|event| event.kind.clone())
                .collect()
        }

        fn connected_and_chat_timeline(&self) -> Vec<String> {
            let connected = serde_json::json!(ConnectionState::Connected);
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter_map(|event| match event.kind.as_str() {
                    PlatformConnectionChanged::KIND if event.payload["state"] == connected => {
                        Some("connected".to_owned())
                    }
                    CHAT_MESSAGE_KIND => event.payload["message"].as_str().map(str::to_owned),
                    _ => None,
                })
                .collect()
        }

        async fn wait_until(&self, what: &str, reached: impl Fn(&Self) -> bool) {
            let arrived = within_real_time(FAKE_WAIT, async {
                while !reached(self) {
                    self.published.notified().await;
                }
            })
            .await;
            assert!(arrived.is_ok(), "{what} never arrived");
        }

        async fn chat_texts_until(&self, last: &str) -> Vec<String> {
            self.wait_until(&format!("the chat message {last:?}"), |bus| {
                bus.chat_texts().iter().any(|text| text == last)
            })
            .await;
            self.chat_texts()
        }
    }

    fn session_over(
        publisher: Arc<dyn EventPublisher>,
        repo: Arc<dyn forge_storage::CredentialsRepo>,
        oauth_base: Option<String>,
        tracker: SubscriptionTracker,
        endpoints: PlatformEndpoints,
    ) -> (
        ChatSession,
        watch::Receiver<ChatConnectionState>,
        oneshot::Sender<()>,
    ) {
        let manager = match oauth_base {
            Some(oauth_base) => TwitchCredentialsManager::new(
                crate::sub_actions::test_support::oauth_client_at(&oauth_base, "client"),
                repo,
            ),
            None => TwitchCredentialsManager::new(
                crate::sub_actions::test_support::unreachable_oauth_client("client"),
                repo,
            ),
        };
        ChatSession::new(
            Arc::new(manager),
            helix_over(&endpoints, Arc::clone(&publisher)),
            session_config(endpoints),
            publisher,
            tracker,
            TwitchLifecycle::new(),
        )
    }

    fn socket_and_api(socket_url: &str, api_url: &str) -> PlatformEndpoints {
        crate::sub_actions::test_support::endpoints_with(&[
            (EndpointSurface::TwitchApi, api_url),
            (EndpointSurface::TwitchEventSubSocket, socket_url),
        ])
    }

    fn chat_over(socket_url: &str, api_url: &str) -> (crate::chat::TwitchChat, Arc<EventChannel>) {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let manager = Arc::new(TwitchCredentialsManager::new(
            crate::sub_actions::test_support::unreachable_oauth_client("client"),
            Arc::new(MockCreds::with_identity()),
        ));
        let endpoints = socket_and_api(socket_url, api_url);
        let chat = crate::chat::TwitchChat::new(
            manager,
            helix_over(&endpoints, bus.clone()),
            session_config(endpoints),
            bus.clone(),
            SubscriptionTracker::default(),
            TwitchLifecycle::new(),
        );
        (chat, bus)
    }

    async fn live_handle(
        base: &mut FakeEventSub,
        api: &wiremock::MockServer,
    ) -> (crate::chat::TwitchChatHandle, Peer, EventSubscription) {
        let (chat, bus) = chat_over(&base.url, &api.uri());
        let mut events = bus.subscribe();
        let handle = chat.start();
        let peer = base.accept().await;
        peer.send(welcome_frame("sess-1"));
        peer.send(chat_frame("first", "on the live session"));
        assert_eq!(next_chat_text(&mut events).await, "on the live session");
        (handle, peer, events)
    }

    const ABORT_GRACE: Duration = Duration::from_millis(150);
    const ABORTED_AFTER_THE_GRACE: &str = "did not stop within the grace period";

    #[test]
    fn shutting_down_a_live_session_closes_its_socket_and_never_redials() {
        let ((), lines) = crate::log_capture::capture_blocking(tracing::Level::WARN, async {
            let api = eventsub_api().await;
            let mut base = FakeEventSub::bind().await;
            let (handle, mut peer, _events) = live_handle(&mut base, &api).await;

            handle.shutdown().await;

            assert!(
                peer.wait_closed_by_forge().await,
                "shutdown must close the socket so Twitch drops the session's subscriptions"
            );
            assert!(
                base.no_dial_pending(),
                "a session that was shut down must not dial a replacement"
            );
        });

        assert!(
            !lines
                .iter()
                .any(|line| line.mentions(ABORTED_AFTER_THE_GRACE)),
            "a healthy session must end on the shutdown request instead of being aborted after \
             the grace period"
        );
    }

    #[test]
    fn shutting_down_while_the_socket_handshake_hangs_ends_the_session() {
        let ((), lines) = crate::log_capture::capture_blocking(tracing::Level::WARN, async {
            let mut hung = HeldHttpEndpoint::bind().await;
            let (chat, bus) = chat_over(&hung.ws_url(), &hung.url);
            let mut events = bus.subscribe();
            let handle = chat.start();
            hung.wait_for_a_request().await;

            handle.shutdown().await;

            connection_states_until(&mut events, ConnectionState::Disconnected).await;
        });

        assert!(
            !lines
                .iter()
                .any(|line| line.mentions(ABORTED_AFTER_THE_GRACE)),
            "a dial that never completes must not hold the session past a shutdown request"
        );
    }

    #[tokio::test]
    async fn shutting_down_during_the_reconnect_backoff_skips_the_rest_of_the_wait() {
        const LONG_BACKOFF: Duration = Duration::from_secs(3_600);
        const HANG_GUARD: Duration = Duration::from_secs(20);
        let dead = unreachable_ws_url();
        let (chat, bus) = chat_over(&dead, &dead.replacen("ws://", "http://", 1));
        let chat = chat.with_timing(ChatTiming {
            reconnect_backoff_base: LONG_BACKOFF,
            reconnect_backoff_cap: LONG_BACKOFF,
            shutdown_grace: LONG_BACKOFF,
            ..ChatTiming::default()
        });
        let mut events = bus.subscribe();
        let handle = chat.start();
        connection_states_until(&mut events, ConnectionState::Connecting).await;

        let shut_down = tokio::time::timeout(HANG_GUARD, handle.shutdown()).await;

        assert!(
            shut_down.is_ok(),
            "a shutdown during the backoff must end the session without sleeping out the delay"
        );
    }

    #[tokio::test]
    async fn dropping_the_chat_handle_ends_its_session() {
        let api = eventsub_api().await;
        let mut base = FakeEventSub::bind().await;
        let (handle, mut peer, _events) = live_handle(&mut base, &api).await;

        drop(handle);

        assert!(
            peer.wait_closed_by_forge().await,
            "a session whose handle is gone can never be stopped again, so it must stop now"
        );
    }

    struct StoreSignallingRepo {
        inner: Arc<crate::credentials_manager::tests::InMemRepo>,
        stored: tokio::sync::Notify,
    }

    #[async_trait::async_trait]
    impl forge_storage::CredentialsRepo for StoreSignallingRepo {
        async fn store(
            &self,
            id: &forge_storage::CredentialId,
            plaintext: &str,
        ) -> Result<(), forge_storage::StorageError> {
            let stored = self.inner.store(id, plaintext).await;
            self.stored.notify_one();
            stored
        }

        async fn load(
            &self,
            id: &forge_storage::CredentialId,
        ) -> Result<Option<String>, forge_storage::StorageError> {
            self.inner.load(id).await
        }

        async fn delete(
            &self,
            id: &forge_storage::CredentialId,
        ) -> Result<bool, forge_storage::StorageError> {
            self.inner.delete(id).await
        }

        async fn list_ids(
            &self,
        ) -> Result<Vec<forge_storage::CredentialId>, forge_storage::StorageError> {
            self.inner.list_ids().await
        }

        async fn last_refresh(
            &self,
            id: &forge_storage::CredentialId,
        ) -> Result<Option<time::OffsetDateTime>, forge_storage::StorageError> {
            self.inner.last_refresh(id).await
        }

        async fn mark_refreshed(
            &self,
            id: &forge_storage::CredentialId,
        ) -> Result<(), forge_storage::StorageError> {
            self.inner.mark_refreshed(id).await
        }
    }

    const ROTATED_ACCESS_TOKEN: &str = "access_rotated_during_shutdown";

    struct AbortedMidRefresh {
        bus: Arc<RecordingBus>,
        state: watch::Receiver<ChatConnectionState>,
        manager: Arc<TwitchCredentialsManager>,
        repo: Arc<StoreSignallingRepo>,
        token_endpoint: HeldHttpEndpoint,
        _peer: Peer,
    }

    async fn shut_down_while_refreshing_the_token() -> AbortedMidRefresh {
        let mut token_endpoint = HeldHttpEndpoint::bind_answering(
            reqwest::StatusCode::OK,
            serde_json::json!({
                "access_token": ROTATED_ACCESS_TOKEN,
                "refresh_token": "refresh_rotated_during_shutdown",
                "expires_in": 14400,
            })
            .to_string(),
        )
        .await;
        let mut base = FakeEventSub::bind().await;
        let near_expiry = std::time::SystemTime::now() + Duration::from_secs(60);
        let repo = Arc::new(StoreSignallingRepo {
            inner: crate::credentials_manager::tests::InMemRepo::seeded(
                &crate::credentials_manager::tests::stub_cred(near_expiry),
            ),
            stored: tokio::sync::Notify::new(),
        });
        let manager = Arc::new(TwitchCredentialsManager::new(
            crate::sub_actions::test_support::oauth_client_at(&token_endpoint.url, "client"),
            repo.clone(),
        ));
        let bus = Arc::new(RecordingBus::default());
        let publisher: Arc<dyn EventPublisher> = bus.clone();
        let endpoints = socket_and_api(&base.url, &token_endpoint.url);
        let (session, state, shutdown) = ChatSession::new(
            Arc::clone(&manager),
            helix_over(&endpoints, Arc::clone(&publisher)),
            session_config(endpoints),
            publisher,
            SubscriptionTracker::default(),
            TwitchLifecycle::new(),
        );
        let handle =
            crate::chat::TwitchChatHandle::spawn(session, state.clone(), shutdown, ABORT_GRACE);
        let peer = base.accept().await;
        peer.send(welcome_frame("sess-1"));
        token_endpoint.wait_for_a_request().await;

        let asked = std::time::Instant::now();
        handle.shutdown().await;
        let waited = asked.elapsed();
        assert!(
            waited >= ABORT_GRACE,
            "the session must be stuck in its token fetch so shutdown has to abort it"
        );

        AbortedMidRefresh {
            bus,
            state,
            manager,
            repo,
            token_endpoint,
            _peer: peer,
        }
    }

    impl RecordingBus {
        fn disconnected_reports(&self) -> usize {
            let disconnected = serde_json::json!(ConnectionState::Disconnected);
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| {
                    event.kind == PlatformConnectionChanged::KIND
                        && event.payload["state"] == disconnected
                })
                .count()
        }
    }

    #[tokio::test]
    async fn a_session_aborted_after_the_grace_period_ends_in_the_disconnected_state() {
        let aborted = shut_down_while_refreshing_the_token().await;

        assert_eq!(
            *aborted.state.borrow(),
            ChatConnectionState::Disconnected,
            "a session cut off mid token fetch must not leave its last reported state behind"
        );
    }

    #[tokio::test]
    async fn a_session_aborted_after_the_grace_period_reports_disconnected_exactly_once() {
        let aborted = shut_down_while_refreshing_the_token().await;

        assert_eq!(
            aborted.bus.disconnected_reports(),
            1,
            "listeners on the bus must learn the session stopped, and only once"
        );
    }

    #[tokio::test]
    async fn a_session_aborted_after_the_grace_period_flags_its_disconnect_expected() {
        let aborted = shut_down_while_refreshing_the_token().await;

        let disconnected = serde_json::json!(ConnectionState::Disconnected);
        let flags: Vec<Option<bool>> = aborted
            .bus
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| {
                event.kind == PlatformConnectionChanged::KIND
                    && event.payload["state"] == disconnected
            })
            .map(|event| event.payload["expected"].as_bool())
            .collect();
        assert_eq!(
            flags,
            [Some(true)],
            "the abort only happens because a shutdown was requested"
        );
    }

    async fn next_link_announcement(
        events: &mut EventSubscription,
    ) -> (ConnectionState, Option<bool>) {
        within_real_time(FAKE_WAIT, async {
            loop {
                let event = events.recv().await.expect("the bus must stay open");
                if event.kind == PlatformConnectionChanged::KIND {
                    return (
                        serde_json::from_value(event.payload["state"].clone())
                            .expect("a connection-state event must carry a known state"),
                        event.payload["expected"].as_bool(),
                    );
                }
            }
        })
        .await
        .expect("the session never announced leaving connected")
    }

    #[derive(Debug, Clone, Copy)]
    enum LeaveConnected {
        ShutdownRequested,
        TwitchClosesTheSocket,
    }

    #[tokio::test]
    async fn leaving_a_live_session_is_flagged_expected_only_when_a_shutdown_was_requested() {
        for (cause, want_state, want_expected) in [
            (
                LeaveConnected::ShutdownRequested,
                ConnectionState::Disconnected,
                true,
            ),
            (
                LeaveConnected::TwitchClosesTheSocket,
                ConnectionState::Reconnecting,
                false,
            ),
        ] {
            let api = eventsub_api().await;
            let mut base = FakeEventSub::bind().await;
            let (handle, peer, mut events) = live_handle(&mut base, &api).await;

            let still_running = match cause {
                LeaveConnected::ShutdownRequested => {
                    handle.shutdown().await;
                    None
                }
                LeaveConnected::TwitchClosesTheSocket => {
                    peer.close();
                    Some(handle)
                }
            };

            assert_eq!(
                next_link_announcement(&mut events).await,
                (want_state, Some(want_expected)),
                "{cause:?}"
            );
            if let Some(handle) = still_running {
                handle.shutdown().await;
            }
        }
    }

    #[tokio::test]
    async fn a_token_refresh_in_flight_when_the_session_is_aborted_still_stores_the_rotated_token()
    {
        let aborted = shut_down_while_refreshing_the_token().await;

        aborted.token_endpoint.release();
        let stored = within_real_time(FAKE_WAIT, aborted.repo.stored.notified()).await;

        assert!(
            stored.is_ok(),
            "Twitch already rotated the refresh token, so the abort must not drop the save"
        );
        let saved = aborted.manager.load().await.unwrap().unwrap();
        assert_eq!(saved.access_token.expose(), ROTATED_ACCESS_TOKEN);
    }

    #[test]
    fn a_graceful_shutdown_reports_disconnected_exactly_once() {
        let ((), lines) = crate::log_capture::capture_blocking(tracing::Level::WARN, async {
            let api = eventsub_api().await;
            let mut base = FakeEventSub::bind().await;
            let bus = Arc::new(RecordingBus::default());
            let (session, state, shutdown) = session_over(
                bus.clone(),
                Arc::new(MockCreds::with_identity()),
                None,
                SubscriptionTracker::default(),
                socket_and_api(&base.url, &api.uri()),
            );
            let handle = crate::chat::TwitchChatHandle::spawn(
                session,
                state,
                shutdown,
                crate::config::SHUTDOWN_GRACE,
            );
            let peer = base.accept().await;
            peer.send(welcome_frame("sess-1"));
            bus.wait_until("the Connected report", |bus| {
                bus.connected_and_chat_timeline()
                    .iter()
                    .any(|entry| entry == "connected")
            })
            .await;

            handle.shutdown().await;

            assert_eq!(
                bus.disconnected_reports(),
                1,
                "a session that stops on request must report Disconnected once, not once per path"
            );
        });

        assert!(
            !lines
                .iter()
                .any(|line| line.mentions(ABORTED_AFTER_THE_GRACE)),
            "a healthy session must end on the shutdown request, not by abort"
        );
    }

    async fn bundle_over(
        socket_url: &str,
        api_url: &str,
    ) -> Arc<crate::builtin::TwitchIntegrationBundle> {
        use forge_platform_core::ChatPlatform;
        let creds: Arc<dyn forge_storage::CredentialsRepo> = Arc::new(MockCreds::with_identity());
        let manager = Arc::new(TwitchCredentialsManager::new(
            crate::sub_actions::test_support::unreachable_oauth_client("client"),
            Arc::clone(&creds),
        ));
        let endpoints = socket_and_api(socket_url, api_url);
        let platform = Arc::new(crate::chat_platform::TwitchPlatform::new(
            session_config(endpoints.clone()),
            creds,
            manager,
            SubscriptionTracker::default(),
            helix_over(
                &endpoints,
                Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY)),
            ),
            TwitchLifecycle::new(),
        ));
        let bundle = crate::builtin::TwitchIntegrationBundle::new(
            Some("streamer".to_owned()),
            Arc::clone(&platform),
        );
        platform.connect().await.unwrap();
        bundle
    }

    async fn bundle_reaches(
        bundle: &crate::builtin::TwitchIntegrationBundle,
        expected: ConnectionState,
    ) -> bool {
        use forge_platform_core::{BuiltinHealth, BuiltinStatus};
        let mut deltas = bundle.stream();
        within_real_time(FAKE_WAIT, async {
            while bundle.connection() != expected {
                if deltas.next().await.is_none() {
                    return false;
                }
            }
            true
        })
        .await
        .unwrap_or(false)
    }

    #[tokio::test]
    async fn reconnect_closes_the_running_session_before_replacing_it() {
        use forge_platform_core::BuiltinControl;
        let api = eventsub_api().await;
        let mut base = FakeEventSub::bind().await;
        let bundle = bundle_over(&base.url, &api.uri()).await;
        let mut first = base.accept().await;
        first.send(welcome_frame("sess-1"));
        assert!(bundle_reaches(&bundle, ConnectionState::Connected).await);

        assert_eq!(bundle.reconnect().await, Ok(()));

        assert!(
            first.wait_closed_by_forge().await,
            "Reconnect must retire the running session; left open, both deliver every event"
        );
        base.accept().await;
    }

    #[tokio::test]
    async fn the_bundle_state_follows_the_session_reconnect_started() {
        use forge_platform_core::BuiltinControl;
        let api = eventsub_api().await;
        let mut base = FakeEventSub::bind().await;
        let bundle = bundle_over(&base.url, &api.uri()).await;
        let first = base.accept().await;
        first.send(welcome_frame("sess-1"));
        assert!(bundle_reaches(&bundle, ConnectionState::Connected).await);
        assert_eq!(bundle.reconnect().await, Ok(()));
        let second = base.accept().await;
        second.send(welcome_frame("sess-2"));
        assert!(
            bundle_reaches(&bundle, ConnectionState::Connected).await,
            "the replacement session's Connected must reach the bundle"
        );

        second.close();

        assert!(
            bundle_reaches(&bundle, ConnectionState::Reconnecting).await,
            "a drop of the replacement session must show on the bundle, not the state of the \
             session Reconnect retired"
        );
    }

    async fn first_retry(state: &mut watch::Receiver<ChatConnectionState>) {
        while !matches!(
            *state.borrow_and_update(),
            ChatConnectionState::Reconnecting { .. }
        ) {
            state
                .changed()
                .await
                .expect("the session task must outlive the state it reports");
        }
    }

    #[tokio::test]
    async fn the_keepalive_deadline_still_ends_a_session_whose_subscription_pass_hangs() {
        let mut helix = HeldHttpEndpoint::bind().await;
        let mut base = FakeEventSub::bind().await;
        let mut session = spawn_session_timed(
            &base.url,
            &helix.url,
            ChatTiming {
                keepalive_timeout: SHORT_KEEPALIVE,
                ..ChatTiming::default()
            },
        );
        let welcomed = base.accept().await;
        welcomed.send(welcome_frame("sess-1"));
        helix.wait_for_a_request().await;

        let retried = tokio::time::timeout(FAKE_WAIT, first_retry(&mut session.state)).await;

        assert!(
            retried.is_ok(),
            "a silent socket must end the session at its keepalive deadline even while the \
             subscription requests are still outstanding"
        );
    }

    const SHORT_KEEPALIVE: Duration = Duration::from_millis(200);
    const HELD_DURING_PASS: usize = 8;

    fn held_text(n: usize) -> String {
        format!("held {n}")
    }

    #[tokio::test]
    async fn frames_arriving_during_the_subscription_pass_publish_in_order_after_connected() {
        let bus = Arc::new(RecordingBus::default());
        let mut helix = HeldHttpEndpoint::bind().await;
        let mut base = FakeEventSub::bind().await;
        let (session, _state, _shutdown) = session_over(
            bus.clone(),
            Arc::new(MockCreds::with_identity()),
            None,
            SubscriptionTracker::default(),
            socket_and_api(&base.url, &helix.url),
        );
        let task = tokio::spawn(session.run());
        let mut welcomed = base.accept().await;
        welcomed.send(welcome_frame("sess-1"));
        helix.wait_for_a_request().await;
        for n in 0..HELD_DURING_PASS {
            welcomed.send(chat_frame(&format!("held-{n}"), &held_text(n)));
        }
        welcomed.wait_until_forge_read_everything().await;
        assert!(
            bus.chat_texts().is_empty(),
            "a frame read while the pass runs must wait for the pass"
        );

        helix.release();
        bus.chat_texts_until(&held_text(HELD_DURING_PASS - 1)).await;

        let expected: Vec<String> = std::iter::once("connected".to_owned())
            .chain((0..HELD_DURING_PASS).map(held_text))
            .collect();
        assert_eq!(
            bus.connected_and_chat_timeline(),
            expected,
            "frames held during the pass must publish after Connected and in arrival order, so \
             consumers still see subscribed-then-events"
        );
        task.abort();
    }

    #[tokio::test]
    async fn frames_past_the_hold_limit_during_a_subscription_pass_are_dropped() {
        let bus = Arc::new(RecordingBus::default());
        let mut helix = HeldHttpEndpoint::bind().await;
        let mut base = FakeEventSub::bind().await;
        let (session, _state, _shutdown) = session_over(
            bus.clone(),
            Arc::new(MockCreds::with_identity()),
            None,
            SubscriptionTracker::default(),
            socket_and_api(&base.url, &helix.url),
        );
        let task = tokio::spawn(session.run());
        let mut welcomed = base.accept().await;
        welcomed.send(welcome_frame("sess-1"));
        helix.wait_for_a_request().await;
        for n in 0..=HELD_FRAME_LIMIT {
            welcomed.send(chat_frame(&format!("held-{n}"), &held_text(n)));
        }
        welcomed.wait_until_forge_read_everything().await;
        helix.release();
        bus.wait_until("Connected", |bus| {
            bus.connected_and_chat_timeline()
                .iter()
                .any(|entry| entry == "connected")
        })
        .await;
        welcomed.send(chat_frame("after", "after connected"));

        let expected: Vec<String> = (0..HELD_FRAME_LIMIT)
            .map(held_text)
            .chain(std::iter::once("after connected".to_owned()))
            .collect();
        assert_eq!(
            bus.chat_texts_until("after connected").await,
            expected,
            "the hold keeps exactly its limit of frames and drops the overflow, so a flood \
             during a stuck pass cannot grow memory without bound"
        );
        task.abort();
    }

    fn revocation_frame(subscription_id: &str, kind: &str, status: &str) -> String {
        serde_json::json!({
            "metadata": {
                "message_id": format!("revocation-{subscription_id}"),
                "message_type": "revocation",
                "message_timestamp": "2026-09-24T12:00:00.000Z",
                "subscription_type": kind,
                "subscription_version": "1",
            },
            "payload": { "subscription": {
                "id": subscription_id,
                "status": status,
                "type": kind,
                "version": "1",
                "cost": 0,
                "condition": { "broadcaster_user_id": "bcast" },
                "transport": { "method": "websocket", "session_id": "sess-1" },
                "created_at": "2026-09-24T11:00:00.000Z",
            }},
        })
        .to_string()
    }

    fn active(
        kind: &str,
        subscription_id: Option<&str>,
    ) -> crate::subscriptions::SubscriptionRecord {
        crate::subscriptions::SubscriptionRecord {
            kind: kind.to_owned(),
            direction: None,
            version: "1".to_owned(),
            status: crate::subscriptions::SubStatus::Active,
            subscription_id: subscription_id.map(str::to_owned),
        }
    }

    fn session_tracking(
        bus: Arc<RecordingBus>,
        records: Vec<crate::subscriptions::SubscriptionRecord>,
    ) -> (ChatSession, SubscriptionTracker) {
        let tracker = SubscriptionTracker::default();
        tracker.reset(records);
        let (session, _state, _shutdown) = session_over(
            bus,
            Arc::new(MockCreds::with_identity()),
            None,
            tracker.clone(),
            crate::sub_actions::test_support::unreachable_twitch_endpoints(),
        );
        (session, tracker)
    }

    #[tokio::test]
    async fn a_revocation_fails_the_tracker_row_it_names_by_id_before_falling_back_to_its_type() {
        use crate::subscriptions::SubStatus;
        const FOLLOW: &str = "channel.follow";
        let revoked = || SubStatus::Failed("revoked: user_removed".to_owned());
        let cases = [
            (
                vec![active(FOLLOW, Some("sub-a")), active(FOLLOW, Some("sub-b"))],
                "sub-b",
                vec![SubStatus::Active, revoked()],
            ),
            (
                vec![
                    active("channel.chat.message", Some("sub-chat")),
                    active(FOLLOW, None),
                ],
                "sub-unknown",
                vec![SubStatus::Active, revoked()],
            ),
        ];

        for (records, subscription_id, expected) in cases {
            let (mut session, tracker) = session_tracking(Arc::default(), records);

            session
                .handle_frame(
                    &revocation_frame(subscription_id, FOLLOW, "user_removed"),
                    &mut Some("sess-1".to_owned()),
                )
                .await;

            let statuses: Vec<SubStatus> = tracker
                .snapshot()
                .iter()
                .map(|record| record.status.clone())
                .collect();
            assert_eq!(
                statuses, expected,
                "revocation of {subscription_id}: the row with that subscription id wins, the \
                 first row of that type is the fallback, every other row keeps its status"
            );
        }
    }

    #[tokio::test]
    async fn an_authorization_revoked_revocation_ends_the_session_and_prompts_reauth() {
        let bus = Arc::new(RecordingBus::default());
        let (mut session, _tracker) = session_tracking(
            Arc::clone(&bus),
            vec![active("channel.chat.message", Some("sub-chat"))],
        );

        let action = session
            .handle_frame(
                &revocation_frame("sub-chat", "channel.chat.message", "authorization_revoked"),
                &mut Some("sess-1".to_owned()),
            )
            .await;

        assert!(
            matches!(action, FrameAction::ReauthRequired),
            "a revoked authorization kills every topic, so the session must stop for sign-in"
        );
        assert_eq!(bus.kinds_published(), vec!["platform.reauth_required"]);
    }

    #[tokio::test]
    async fn a_revocation_that_leaves_the_authorization_intact_keeps_the_session_running() {
        for status in ["user_removed", "version_removed"] {
            let bus = Arc::new(RecordingBus::default());
            let (mut session, _tracker) = session_tracking(
                Arc::clone(&bus),
                vec![active("channel.chat.message", Some("sub-chat"))],
            );

            let action = session
                .handle_frame(
                    &revocation_frame("sub-chat", "channel.chat.message", status),
                    &mut Some("sess-1".to_owned()),
                )
                .await;

            assert!(
                matches!(action, FrameAction::Continue),
                "{status}: one dead topic must not stop the others"
            );
            assert!(
                bus.kinds_published().is_empty(),
                "{status}: nothing about the account changed, so no sign-in prompt"
            );
        }
    }

    const STALE_ACCESS: &str = "stale_access";
    const RENEWED_ACCESS: &str = "renewed_access";
    const FORBIDDEN_TOPIC: &str = "channel.follow";

    #[derive(Clone, Copy, Debug)]
    enum TopicReply {
        RejectsTheStaleTokenOnly,
        RejectsEveryToken,
        ForbidsOneTopicAndRejectsTheRest,
    }

    #[derive(Clone, Copy, Debug)]
    enum TokenEndpoint {
        Rotates,
        RejectsTheRefresh,
        Unreachable,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Verdict {
        Live,
        NothingLive,
        TokenUnavailable,
        ReauthRequired,
    }

    impl From<PassVerdict> for Verdict {
        fn from(verdict: PassVerdict) -> Self {
            match verdict {
                PassVerdict::Live => Self::Live,
                PassVerdict::NothingLive => Self::NothingLive,
                PassVerdict::TokenUnavailable => Self::TokenUnavailable,
                PassVerdict::ReauthRequired => Self::ReauthRequired,
            }
        }
    }

    fn bearer(token: &str) -> wiremock::matchers::HeaderExactMatcher {
        wiremock::matchers::header("authorization", format!("Bearer {token}").as_str())
    }

    async fn mount_topic_replies(server: &wiremock::MockServer, reply: TopicReply) {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, ResponseTemplate};
        let unauthorized = || ResponseTemplate::new(reqwest::StatusCode::UNAUTHORIZED.as_u16());
        match reply {
            TopicReply::RejectsTheStaleTokenOnly => {
                Mock::given(method("POST"))
                    .and(path(EVENTSUB_SUBSCRIPTIONS_PATH))
                    .and(bearer(RENEWED_ACCESS))
                    .respond_with(subscription_accepted())
                    .mount(server)
                    .await;
            }
            TopicReply::RejectsEveryToken => {}
            TopicReply::ForbidsOneTopicAndRejectsTheRest => {
                Mock::given(method("POST"))
                    .and(path(EVENTSUB_SUBSCRIPTIONS_PATH))
                    .and(body_string_contains(format!(
                        r#""type":"{FORBIDDEN_TOPIC}""#
                    )))
                    .respond_with(ResponseTemplate::new(
                        reqwest::StatusCode::FORBIDDEN.as_u16(),
                    ))
                    .with_priority(1)
                    .mount(server)
                    .await;
            }
        }
        Mock::given(method("POST"))
            .and(path(EVENTSUB_SUBSCRIPTIONS_PATH))
            .respond_with(unauthorized())
            .mount(server)
            .await;
    }

    async fn pass_after_topic_replies(
        reply: TopicReply,
        endpoint: TokenEndpoint,
    ) -> (Verdict, usize) {
        let (verdict, requests) = requests_of_a_pass(reply, endpoint).await;
        let refreshes = requests
            .iter()
            .filter(|request| request.url.path() == "/token")
            .count();
        (verdict, refreshes)
    }

    async fn requests_of_a_pass(
        reply: TopicReply,
        endpoint: TokenEndpoint,
    ) -> (Verdict, Vec<wiremock::Request>) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};
        let server = wiremock::MockServer::start().await;
        mount_topic_replies(&server, reply).await;
        let token_url = match endpoint {
            TokenEndpoint::Rotates => {
                Mock::given(method("POST"))
                    .and(path("/token"))
                    .respond_with(
                        ResponseTemplate::new(reqwest::StatusCode::OK.as_u16()).set_body_json(
                            serde_json::json!({
                                "access_token": RENEWED_ACCESS,
                                "refresh_token": "rotated_refresh",
                                "expires_in": 14400,
                            }),
                        ),
                    )
                    .mount(&server)
                    .await;
                server.uri()
            }
            TokenEndpoint::RejectsTheRefresh => {
                Mock::given(method("POST"))
                    .and(path("/token"))
                    .respond_with(
                        ResponseTemplate::new(reqwest::StatusCode::BAD_REQUEST.as_u16())
                            .set_body_json(serde_json::json!({ "error": "invalid_grant" })),
                    )
                    .mount(&server)
                    .await;
                server.uri()
            }
            TokenEndpoint::Unreachable => unreachable_ws_url().replacen("ws://", "http://", 1),
        };
        let mut stored = crate::credentials_manager::tests::stub_cred(
            std::time::SystemTime::now() + Duration::from_secs(3600),
        );
        stored.access_token = forge_types::OAuthToken::new(STALE_ACCESS);
        let (mut session, _state, _shutdown) = session_over(
            Arc::new(RecordingBus::default()),
            crate::credentials_manager::tests::InMemRepo::seeded(&stored),
            Some(token_url),
            SubscriptionTracker::default(),
            socket_and_api("ws://127.0.0.1:1", &server.uri()),
        );

        let pass = match session
            .handle_frame(&welcome_frame("sess-1"), &mut None)
            .await
        {
            FrameAction::Subscribe(pass) => Some(pass),
            _ => None,
        };
        let pass = pass.expect("a welcome with a usable token must start a subscription pass");
        let verdict = Verdict::from(pass.await);
        (verdict, server.received_requests().await.unwrap())
    }

    fn topic_posts_by_token(requests: &[wiremock::Request], token: &str) -> Vec<String> {
        let authorization = format!("Bearer {token}");
        requests
            .iter()
            .filter(|request| request.url.path() == EVENTSUB_SUBSCRIPTIONS_PATH)
            .filter(|request| {
                request
                    .headers
                    .get("authorization")
                    .is_some_and(|value| value.as_bytes() == authorization.as_bytes())
            })
            .map(|request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                format!("{}|{}", body["type"], body["condition"])
            })
            .collect()
    }

    #[tokio::test]
    async fn a_subscription_pass_rejecting_the_token_refreshes_once_and_follows_the_refresh() {
        let cases = [
            (
                TopicReply::RejectsTheStaleTokenOnly,
                TokenEndpoint::Rotates,
                Verdict::Live,
                1,
            ),
            (
                TopicReply::RejectsEveryToken,
                TokenEndpoint::Rotates,
                Verdict::ReauthRequired,
                1,
            ),
            (
                TopicReply::RejectsTheStaleTokenOnly,
                TokenEndpoint::RejectsTheRefresh,
                Verdict::ReauthRequired,
                1,
            ),
            (
                TopicReply::RejectsTheStaleTokenOnly,
                TokenEndpoint::Unreachable,
                Verdict::TokenUnavailable,
                0,
            ),
            (
                TopicReply::ForbidsOneTopicAndRejectsTheRest,
                TokenEndpoint::Rotates,
                Verdict::ReauthRequired,
                1,
            ),
        ];

        for (reply, endpoint, expected, expected_refreshes) in cases {
            let outcome = pass_after_topic_replies(reply, endpoint).await;

            assert_eq!(
                outcome,
                (expected, expected_refreshes),
                "{reply:?} with a token endpoint that {endpoint:?}: a 401 earns exactly one \
                 refresh and retry even beside a 403, and only a token still rejected after \
                 it asks for sign-in"
            );
        }
    }

    #[tokio::test]
    async fn a_token_rejected_pass_posts_every_topic_once_per_token_without_per_topic_retries() {
        let (_, requests) =
            requests_of_a_pass(TopicReply::RejectsEveryToken, TokenEndpoint::Rotates).await;

        let stale = topic_posts_by_token(&requests, STALE_ACCESS);
        let renewed = topic_posts_by_token(&requests, RENEWED_ACCESS);
        let distinct = |posts: &[String]| {
            posts
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        };
        assert!(!stale.is_empty());
        assert_eq!(
            (distinct(&stale), renewed.len(), distinct(&renewed)),
            (stale.len(), stale.len(), stale.len()),
            "the pass must restart once on the renewed token, each topic posted once per token"
        );
    }

    #[tokio::test]
    async fn a_session_whose_pass_is_still_rejected_after_refresh_stops_and_prompts_reauth() {
        let api = wiremock::MockServer::start().await;
        mount_topic_replies(&api, TopicReply::RejectsEveryToken).await;
        let mut base = FakeEventSub::bind().await;
        let mut session = spawn_session(&base.url, &api.uri());
        let mut events = session.bus.subscribe();
        let welcomed = base.accept().await;

        welcomed.send(welcome_frame("sess-1"));

        next_event(&mut events, "platform.reauth_required").await;
        let ended = within_real_time(FAKE_WAIT, &mut session.task).await;
        assert!(
            ended.is_ok(),
            "a session that needs sign-in must stop instead of retrying a token Twitch refuses"
        );
        assert!(base.no_dial_pending());
    }

    #[tokio::test]
    async fn a_chat_message_publishes_exactly_the_shared_fixture_payload() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/published_chat_message.json"
        ))
        .unwrap();
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let session = make_session(&bus);
        let sub = bus.subscribe();

        session.publish_chat_message(&fixture["raw"]);
        let ev = published(sub).await;

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

    const RETRIED_TOPIC: &str = "stream.online";

    fn topic_post(kind: &str) -> wiremock::MockBuilder {
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
            .and(wiremock::matchers::body_string_contains(format!(
                r#""type":"{kind}""#
            )))
    }

    fn topic_post_on_session(kind: &str, session_id: &str) -> wiremock::MockBuilder {
        topic_post(kind).and(wiremock::matchers::body_string_contains(format!(
            r#""session_id":"{session_id}""#
        )))
    }

    fn status_reply(status: reqwest::StatusCode) -> wiremock::ResponseTemplate {
        wiremock::ResponseTemplate::new(status.as_u16())
    }

    async fn mount_accept_every_other_topic(server: &wiremock::MockServer) {
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(EVENTSUB_SUBSCRIPTIONS_PATH))
            .respond_with(subscription_accepted())
            .mount(server)
            .await;
    }

    fn spawn_tracked_session(
        socket_url: &str,
        api_url: &str,
    ) -> (RunningSession, SubscriptionTracker) {
        let bus = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        let tracker = SubscriptionTracker::default();
        let publisher: Arc<dyn EventPublisher> = bus.clone();
        let (session, state, shutdown) = session_over(
            publisher,
            Arc::new(MockCreds::with_identity()),
            None,
            tracker.clone(),
            socket_and_api(socket_url, api_url),
        );
        let running = RunningSession {
            bus,
            state,
            task: tokio::spawn(session.run()),
            _shutdown: shutdown,
        };
        (running, tracker)
    }

    fn tracked_status(
        tracker: &SubscriptionTracker,
        kind: &str,
    ) -> crate::subscriptions::SubStatus {
        tracker
            .snapshot()
            .iter()
            .find(|record| record.kind == kind)
            .map(|record| record.status.clone())
            .expect("the pass must have recorded every topic")
    }

    async fn next_created(events: &mut EventSubscription, kind: &str) -> Event {
        let found = within_real_time(FAKE_WAIT, async {
            loop {
                let event = events.recv().await.expect("the bus must stay open");
                if event.kind == crate::vocabulary::event_kinds::EVENTSUB_SUBSCRIPTION_CREATED
                    && event.payload["type"] == kind
                {
                    return event;
                }
            }
        })
        .await;
        found.expect("the retried topic must be announced once it subscribes")
    }

    #[tokio::test]
    async fn a_retry_pending_across_a_successor_swap_subscribes_on_the_successor_session() {
        let api = wiremock::MockServer::start().await;
        topic_post_on_session(RETRIED_TOPIC, "sess-1")
            .respond_with(status_reply(reqwest::StatusCode::INTERNAL_SERVER_ERROR))
            .with_priority(1)
            .mount(&api)
            .await;
        topic_post_on_session(RETRIED_TOPIC, "sess-2")
            .respond_with(status_reply(reqwest::StatusCode::ACCEPTED).set_body_json(
                serde_json::json!({
                    "data": [{ "id": "sub-on-successor", "type": RETRIED_TOPIC, "condition": {} }]
                }),
            ))
            .with_priority(1)
            .mount(&api)
            .await;
        mount_accept_every_other_topic(&api).await;
        let mut base = FakeEventSub::bind().await;
        let (session, _tracker) = spawn_tracked_session(&base.url, &api.uri());
        let mut events = session.bus.subscribe();
        let predecessor = base.accept().await;
        predecessor.send(welcome_frame("sess-1"));
        predecessor.send(chat_frame("first", "before the swap"));
        assert_eq!(next_chat_text(&mut events).await, "before the swap");
        let mut successor_socket = FakeEventSub::bind().await;
        predecessor.send(reconnect_frame(&successor_socket.url));
        let successor = successor_socket.accept().await;

        successor.send(welcome_frame("sess-2"));

        assert_eq!(
            next_created(&mut events, RETRIED_TOPIC).await.payload["subscription_id"],
            "sub-on-successor",
            "Twitch carries only the live subscriptions to the successor, so a topic still \
             retrying must be created against the successor's session id"
        );
    }

    #[tokio::test]
    async fn a_session_ending_while_a_topic_retries_leaves_that_topic_failed() {
        let api = wiremock::MockServer::start().await;
        topic_post(RETRIED_TOPIC)
            .respond_with(status_reply(reqwest::StatusCode::INTERNAL_SERVER_ERROR))
            .with_priority(1)
            .mount(&api)
            .await;
        mount_accept_every_other_topic(&api).await;
        let mut base = FakeEventSub::bind().await;
        let (mut session, tracker) = spawn_tracked_session(&base.url, &api.uri());
        let mut events = session.bus.subscribe();
        let peer = base.accept().await;
        peer.send(welcome_frame("sess-1"));
        peer.send(chat_frame("first", "while the topic retries"));
        assert_eq!(next_chat_text(&mut events).await, "while the topic retries");
        assert_eq!(
            tracked_status(&tracker, RETRIED_TOPIC),
            crate::subscriptions::SubStatus::Retrying("HTTP 500".to_owned())
        );

        peer.close();
        next_retry_attempt(&mut session.state).await;

        assert_eq!(
            tracked_status(&tracker, RETRIED_TOPIC),
            crate::subscriptions::SubStatus::Failed("HTTP 500".to_owned()),
            "the retry ends with the socket session that owns it, so its row must not keep \
             promising a retry"
        );
    }

    #[tokio::test]
    async fn a_pass_that_does_not_go_live_leaves_no_topic_retrying() {
        for (rest_of_topics, expected) in [
            (
                reqwest::StatusCode::SERVICE_UNAVAILABLE,
                Verdict::NothingLive,
            ),
            (reqwest::StatusCode::FORBIDDEN, Verdict::ReauthRequired),
        ] {
            let api = wiremock::MockServer::start().await;
            topic_post(RETRIED_TOPIC)
                .respond_with(status_reply(reqwest::StatusCode::INTERNAL_SERVER_ERROR))
                .with_priority(1)
                .mount(&api)
                .await;
            wiremock::Mock::given(wiremock::matchers::method("POST"))
                .respond_with(status_reply(rest_of_topics))
                .mount(&api)
                .await;
            let tracker = SubscriptionTracker::default();
            let (mut session, _state, _shutdown) = session_over(
                Arc::new(RecordingBus::default()),
                Arc::new(MockCreds::with_identity()),
                None,
                tracker.clone(),
                socket_and_api("ws://127.0.0.1:1", &api.uri()),
            );
            let pass = match session
                .handle_frame(&welcome_frame("sess-1"), &mut None)
                .await
            {
                FrameAction::Subscribe(pass) => Some(pass),
                _ => None,
            }
            .expect("a welcome with a usable token must start a subscription pass");

            let verdict = Verdict::from(pass.await);

            assert_eq!(
                (verdict, tracked_status(&tracker, RETRIED_TOPIC)),
                (
                    expected,
                    crate::subscriptions::SubStatus::Failed("HTTP 500".to_owned())
                )
            );
        }
    }

    #[tokio::test]
    async fn a_scope_rejection_prompts_sign_in_from_the_pass_only_while_other_topics_stay_live() {
        let cases = [
            (
                true,
                Verdict::Live,
                vec!["platform.reauth_required".to_owned()],
            ),
            (false, Verdict::ReauthRequired, Vec::new()),
        ];
        for (others_accepted, expected, published) in cases {
            let api = wiremock::MockServer::start().await;
            if others_accepted {
                topic_post("channel.follow")
                    .respond_with(status_reply(reqwest::StatusCode::FORBIDDEN))
                    .with_priority(1)
                    .mount(&api)
                    .await;
                mount_accept_every_other_topic(&api).await;
            } else {
                wiremock::Mock::given(wiremock::matchers::method("POST"))
                    .respond_with(status_reply(reqwest::StatusCode::FORBIDDEN))
                    .mount(&api)
                    .await;
            }
            let bus = Arc::new(RecordingBus::default());
            let (mut session, _state, _shutdown) = session_over(
                bus.clone(),
                Arc::new(MockCreds::with_identity()),
                None,
                SubscriptionTracker::default(),
                socket_and_api("ws://127.0.0.1:1", &api.uri()),
            );
            let pass = match session
                .handle_frame(&welcome_frame("sess-1"), &mut None)
                .await
            {
                FrameAction::Subscribe(pass) => Some(pass),
                _ => None,
            }
            .expect("a welcome with a usable token must start a subscription pass");

            let verdict = Verdict::from(pass.await);

            let reauth_prompts: Vec<String> = bus
                .kinds_published()
                .into_iter()
                .filter(|kind| kind == "platform.reauth_required")
                .collect();
            assert_eq!((verdict, reauth_prompts), (expected, published));
        }
    }

    #[tokio::test]
    async fn a_retry_rejected_as_unauthorized_ends_the_topic_without_refreshing_the_token() {
        let server = wiremock::MockServer::start().await;
        topic_post(RETRIED_TOPIC)
            .respond_with(status_reply(reqwest::StatusCode::INTERNAL_SERVER_ERROR))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        topic_post(RETRIED_TOPIC)
            .respond_with(status_reply(reqwest::StatusCode::UNAUTHORIZED))
            .with_priority(2)
            .mount(&server)
            .await;
        mount_accept_every_other_topic(&server).await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                status_reply(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "access_token": RENEWED_ACCESS,
                    "refresh_token": "rotated_refresh",
                    "expires_in": 14400,
                })),
            )
            .mount(&server)
            .await;
        let mut stored = crate::credentials_manager::tests::stub_cred(
            std::time::SystemTime::now() + Duration::from_secs(3600),
        );
        stored.access_token = forge_types::OAuthToken::new(STALE_ACCESS);
        let tracker = SubscriptionTracker::default();
        let (mut session, _state, _shutdown) = session_over(
            Arc::new(RecordingBus::default()),
            crate::credentials_manager::tests::InMemRepo::seeded(&stored),
            Some(server.uri()),
            tracker.clone(),
            socket_and_api("ws://127.0.0.1:1", &server.uri()),
        );
        let pass = match session
            .handle_frame(&welcome_frame("sess-1"), &mut None)
            .await
        {
            FrameAction::Subscribe(pass) => Some(pass),
            _ => None,
        }
        .expect("a welcome with a usable token must start a subscription pass");
        assert_eq!(Verdict::from(pass.await), Verdict::Live);
        let mut retries = session
            .retries_rx
            .take()
            .and_then(|mut handoff| handoff.try_recv().ok())
            .and_then(TopicRetries::into_run)
            .expect("a live pass with a failed topic must hand its retries to the session");

        let finished = within_real_time(FAKE_WAIT, retries.finished()).await;

        let refreshes = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == "/token")
            .count();
        assert_eq!(
            (
                finished.is_ok(),
                refreshes,
                tracked_status(&tracker, RETRIED_TOPIC)
            ),
            (
                true,
                0,
                crate::subscriptions::SubStatus::Failed("unauthorized".to_owned())
            ),
            "a retry keeps the pass's token: a 401 closes the topic and never spends a refresh"
        );
    }

    #[tokio::test]
    async fn a_revocation_of_a_raid_topic_recovered_without_an_id_fails_its_direction_row() {
        use crate::subscriptions::{SubStatus, SubscriptionRecord};
        let raid_row = |direction| SubscriptionRecord {
            direction: Some(direction),
            ..active("channel.raid", None)
        };
        let revoked = || SubStatus::Failed("revoked: user_removed".to_owned());
        let cases = [
            ("to_broadcaster_user_id", vec![revoked(), SubStatus::Active]),
            (
                "from_broadcaster_user_id",
                vec![SubStatus::Active, revoked()],
            ),
        ];

        for (condition_field, expected) in cases {
            let (mut session, tracker) = session_tracking(
                Arc::default(),
                vec![
                    raid_row(RaidDirection::Received),
                    raid_row(RaidDirection::Sent),
                ],
            );
            let mut frame: serde_json::Value = serde_json::from_str(&revocation_frame(
                "sub-unlisted",
                "channel.raid",
                "user_removed",
            ))
            .unwrap();
            frame["payload"]["subscription"]["condition"] =
                serde_json::json!({ condition_field: "bcast" });

            session
                .handle_frame(&frame.to_string(), &mut Some("sess-1".to_owned()))
                .await;

            let statuses: Vec<SubStatus> = tracker
                .snapshot()
                .iter()
                .map(|record| record.status.clone())
                .collect();
            assert_eq!(statuses, expected, "revocation carrying {condition_field}");
        }
    }
}
