use crate::bot_account::TwitchBotAccount;
use crate::chat::subscriber::{SubscribePass, TopicRetries, TopicRetryRun, subscribe_all};
use crate::chat::topic_catalog::raid_direction;
use crate::config::{ChatSessionConfig, ChatTiming};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::credentials_manager::warn_token_failure;
use crate::endpoint_surfaces::TWITCH_EVENTSUB_SOCKET;
use crate::error::TwitchError;
use crate::helix::{FixedTokenSource, HelixHttpTransport, HelixTransport};
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::SubscriptionTracker;
use crate::vocabulary::{self, PlatformReauthRequired, RaidDirection};
use forge_events::{Event, EventPublisher, EventSource, TypedEvent};
use forge_net::{Backoff, Gateway};
use forge_platform_core::{
    ConnectionState, ConnectionTransition, CredentialState, PlatformEndpoints, PlatformError,
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

mod publish_chat;
mod publish_moderation;
mod publish_polls;
mod publish_rewards;
mod publish_stream;
mod socket;
mod subscription_pass;
mod wire;

use self::socket::*;
use self::subscription_pass::*;
use self::wire::*;

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

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SUCCESSOR_TIMEOUT: Duration = Duration::from_secs(15);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const PREDECESSOR_DRAIN_LIMIT: usize = 256;
const HELD_FRAME_LIMIT: usize = 1024;
const AUTHORIZATION_REVOKED_STATUS: &str = "authorization_revoked";
const MESSAGE_SESSION_WELCOME: &str = "session_welcome";
const MESSAGE_SESSION_KEEPALIVE: &str = "session_keepalive";
const MESSAGE_SESSION_RECONNECT: &str = "session_reconnect";
const MESSAGE_NOTIFICATION: &str = "notification";
const MESSAGE_REVOCATION: &str = "revocation";

type EventSubSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

type SuccessorDial =
    Pin<Box<dyn Future<Output = Result<(EventSubSocket, String), TwitchError>> + Send>>;

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
            .base_url(TWITCH_EVENTSUB_SOCKET)
            .to_owned();
        loop {
            let attempt = self.backoff.attempt();
            self.set_state(if attempt == 0 {
                ChatConnectionState::Connecting
            } else {
                ChatConnectionState::Reconnecting {
                    attempt: u8::try_from(attempt).unwrap_or(u8::MAX),
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
                retry_in_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
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
                            self.apply_pass_live();
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
                            let action = self.handle_frame(&text, &mut session_id).await;
                            keepalive_deadline = Instant::now() + self.keepalive_timeout;
                            match action {
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
            MESSAGE_SESSION_WELCOME => return self.on_welcome(&frame, session_id),

            MESSAGE_SESSION_KEEPALIVE => {
                trace!("keepalive received");
            }

            MESSAGE_SESSION_RECONNECT => return on_reconnect(&frame),

            MESSAGE_NOTIFICATION => return self.on_notification(&frame),

            MESSAGE_REVOCATION => return self.on_revocation(&frame),

            other => {
                debug!(message_type = %other, "unhandled WS message type");
            }
        }

        FrameAction::Continue
    }

    fn on_welcome(&mut self, frame: &WsFrame, session_id: &mut Option<String>) -> FrameAction {
        let id = match frame.payload.as_ref().and_then(|p| p.session.as_ref()) {
            Some(s) => s.id.clone(),
            None => {
                warn!("session_welcome missing session.id");
                return FrameAction::Disconnect;
            }
        };
        debug!("session_welcome received, subscribing topics");
        *session_id = Some(id.clone());
        if let Some(seconds) = frame
            .payload
            .as_ref()
            .and_then(|p| p.session.as_ref())
            .and_then(|s| s.keepalive_timeout_seconds)
        {
            self.keepalive_timeout = Duration::from_secs(seconds) + crate::config::KEEPALIVE_GRACE;
        }

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
        FrameAction::Subscribe(Box::pin(pass.run()))
    }

    fn on_notification(&mut self, frame: &WsFrame) -> FrameAction {
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
            self.config.tracker.record_event(sub_type);
            match dispatch::route_for(sub_type) {
                Some(route) => route(self, event_data, frame_msg_id),
                None => {
                    debug!(subscription_type = %sub_type, "no route registered for notification subscription type");
                }
            }
        }
        FrameAction::Continue
    }

    fn on_revocation(&self, frame: &WsFrame) -> FrameAction {
        let Some(revoked) = frame.payload.as_ref().and_then(|p| p.subscription.as_ref()) else {
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
        FrameAction::Continue
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

    fn apply_pass_live(&self) {
        self.config
            .manager
            .credential_state()
            .report(CredentialState::Valid);
    }

    fn publish_reauth_required(&self) {
        self.config
            .manager
            .credential_state()
            .report(CredentialState::ReauthRequired);
        PlatformReauthRequired {
            platform_id: vocabulary::PLATFORM,
        }
        .publish(self.config.bus.as_ref(), EventSource::Twitch, None);
    }
}

fn on_reconnect(frame: &WsFrame) -> FrameAction {
    let new_url = frame
        .payload
        .as_ref()
        .and_then(|p| p.session.as_ref())
        .and_then(|s| s.reconnect_url.clone());
    match new_url {
        Some(new_url) => {
            info!("server-initiated reconnect requested");
            FrameAction::Reconnect(new_url)
        }
        None => {
            warn!("session_reconnect missing reconnect_url");
            FrameAction::Disconnect
        }
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
    keepalive_timeout_seconds: Option<u64>,
}

#[cfg(test)]
mod tests;
