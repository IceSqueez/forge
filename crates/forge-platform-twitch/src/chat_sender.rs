use std::collections::BTreeMap;
use std::sync::Arc;

use forge_events::{EventPublisher, EventSource, TypedEvent};
use forge_platform_core::{ActingSender, SENT_AS_FIELD, SenderPreference, SenderRole};
use forge_types::SubActionOutcome;
use thiserror::Error;
use tracing::warn;

use crate::account::identity::SelfIdentity;
use crate::account::user_target::UserTarget;
use crate::bot_account::{BotIdentity, BotModerator, BotUnavailable, TwitchBotAccount};
use crate::cadence::CadenceLedger;
use crate::chat::refusal_text;
use crate::chat::{
    ChatSendError, WhisperError, send_chat, send_chat_reply, send_whisper, send_whisper_to,
};
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{FallbackLevel, SenderFallback};

const ANNOUNCEMENTS_PATH: &str = "/helix/chat/announcements";
const SHOUTOUTS_PATH: &str = "/helix/chat/shoutouts";

#[derive(Debug, Error)]
pub(crate) enum SendFailure {
    #[error("bot not connected ({0})")]
    BotNotConnected(BotUnavailable),
    #[error("the bot account has no verified phone number, so Twitch refused the whisper")]
    BotPhoneUnverified,
    #[error(transparent)]
    Chat(ChatSendError),
    #[error(transparent)]
    Whisper(WhisperError),
    #[error("bot is not a moderator - make it one in Integrations -> Twitch")]
    BotNotModerator,
    #[error("Twitch refused: {0}")]
    PermissionDenied(String),
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Helix(HelixError),
    #[error("{0}")]
    Credentials(String),
}

impl SendFailure {
    fn token_rejected(&self) -> bool {
        matches!(
            self,
            Self::Chat(ChatSendError::ReauthRequired)
                | Self::Whisper(WhisperError::Helix(HelixError::ReauthRequired))
                | Self::Helix(HelixError::ReauthRequired)
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ChatDelivery<'a> {
    Message {
        text: &'a str,
    },
    Reply {
        text: &'a str,
        parent_message_id: &'a str,
    },
    Announcement {
        text: &'a str,
        color: &'a str,
    },
    Shoutout {
        target: UserTarget<'a>,
    },
    WhisperToLogin {
        login: &'a str,
        text: &'a str,
    },
    WhisperToTarget {
        recipient: Option<UserTarget<'a>>,
        text: &'a str,
    },
}

impl ChatDelivery<'_> {
    const fn operation(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::Reply { .. } => "reply",
            Self::Announcement { .. } => "announcement",
            Self::Shoutout { .. } => "shoutout",
            Self::WhisperToLogin { .. } | Self::WhisperToTarget { .. } => "whisper",
        }
    }

    const fn needs_moderator(&self) -> bool {
        matches!(self, Self::Announcement { .. } | Self::Shoutout { .. })
    }

    const fn is_whisper(&self) -> bool {
        matches!(
            self,
            Self::WhisperToLogin { .. } | Self::WhisperToTarget { .. }
        )
    }
}

struct Actor {
    role: SenderRole,
    user_id: String,
    login: String,
    transport: Arc<dyn HelixTransport>,
}

impl Actor {
    fn acting_sender(&self) -> ActingSender {
        ActingSender {
            role: self.role,
            login: self.login.clone(),
        }
    }

    fn from_bot(bot: BotIdentity) -> Self {
        Self {
            role: SenderRole::Bot,
            user_id: bot.user_id,
            login: bot.login,
            transport: bot.transport,
        }
    }
}

pub(crate) struct ChatSenders {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
    bot: Option<Arc<TwitchBotAccount>>,
    cadence: Arc<CadenceLedger>,
}

impl ChatSenders {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
            bot: None,
            cadence: Arc::default(),
        }
    }

    pub(crate) fn with_cadence(mut self, cadence: Arc<CadenceLedger>) -> Self {
        self.cadence = cadence;
        self
    }

    pub(crate) fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.bot = bot;
        self
    }

    pub(crate) async fn deliver(
        &self,
        delivery: ChatDelivery<'_>,
        preference: SenderPreference,
        events: &dyn EventPublisher,
    ) -> Result<ActingSender, SendFailure> {
        let stored = self.identity.stored().await.map_err(|e| match e {
            HelixError::Credentials(reason) => SendFailure::Credentials(reason),
            other => SendFailure::Chat(ChatSendError::from(other)),
        })?;
        let broadcaster_id = stored.user_id.clone();
        let streamer = Actor {
            role: SenderRole::Streamer,
            user_id: stored.user_id,
            login: stored.login,
            transport: Arc::clone(&self.transport),
        };
        match preference {
            SenderPreference::Streamer => self.post(&streamer, &broadcaster_id, delivery).await,
            SenderPreference::BotOnly => self.send_as_bot(&broadcaster_id, delivery).await,
            SenderPreference::Default => match self.send_as_bot(&broadcaster_id, delivery).await {
                Err(SendFailure::BotNotConnected(reason)) => {
                    if reason != BotUnavailable::NotConnected {
                        announce_fallback(events, reason, &delivery);
                    }
                    self.post(&streamer, &broadcaster_id, delivery).await
                }
                other => other,
            },
        }
    }

    async fn send_as_bot(
        &self,
        broadcaster_id: &str,
        delivery: ChatDelivery<'_>,
    ) -> Result<ActingSender, SendFailure> {
        let Some(bot_account) = &self.bot else {
            return Err(SendFailure::BotNotConnected(BotUnavailable::NotConnected));
        };
        let bot = Actor::from_bot(
            bot_account
                .identity()
                .await
                .map_err(SendFailure::BotNotConnected)?,
        );
        if delivery.needs_moderator()
            && bot_account.moderator_status() == BotModerator::NotModerator
        {
            return Err(SendFailure::BotNotModerator);
        }
        match self.post(&bot, broadcaster_id, delivery).await {
            Err(SendFailure::PermissionDenied(reason)) if delivery.needs_moderator() => {
                bot_account.refresh_moderator_status().await;
                Err(SendFailure::PermissionDenied(reason))
            }
            Err(failure) if failure.token_rejected() => {
                if delivery.is_whisper() && !bot_account.token_confirmed_invalid().await {
                    return Err(SendFailure::BotPhoneUnverified);
                }
                bot_account.mark_reauth_required();
                Err(SendFailure::BotNotConnected(BotUnavailable::ReauthRequired))
            }
            other => other,
        }
    }
}

impl ChatSenders {
    async fn post(
        &self,
        actor: &Actor,
        broadcaster_id: &str,
        delivery: ChatDelivery<'_>,
    ) -> Result<ActingSender, SendFailure> {
        let transport = actor.transport.as_ref();
        match delivery {
            ChatDelivery::Message { text } => {
                send_chat(transport, broadcaster_id, &actor.user_id, text)
                    .await
                    .map(drop)
                    .map_err(SendFailure::Chat)?;
            }
            ChatDelivery::Reply {
                text,
                parent_message_id,
            } => {
                send_chat_reply(
                    transport,
                    broadcaster_id,
                    &actor.user_id,
                    text,
                    parent_message_id,
                )
                .await
                .map(drop)
                .map_err(SendFailure::Chat)?;
            }
            ChatDelivery::Announcement { text, color } => {
                let reservation = self.cadence.reserve_announcement();
                if !reservation.wait().is_zero() {
                    tokio::time::sleep(reservation.wait()).await;
                }
                let request = HelixRequest::new(HelixMethod::Post, ANNOUNCEMENTS_PATH)
                    .query("broadcaster_id", broadcaster_id)
                    .query("moderator_id", actor.user_id.as_str())
                    .body(serde_json::json!({ "message": text, "color": color }));
                transport
                    .execute(request)
                    .await
                    .map_err(moderation_failure)?;
                reservation.commit();
            }
            ChatDelivery::Shoutout { target } => {
                let requested_key = target_key(target);
                self.refuse_during_shoutout_cooldown(&[&requested_key])?;
                let target_id = target
                    .user_id(transport)
                    .await
                    .map_err(moderation_failure)?;
                let id_key = target_key(UserTarget::Id(&target_id));
                self.refuse_during_shoutout_cooldown(&[&id_key])?;
                let request = HelixRequest::new(HelixMethod::Post, SHOUTOUTS_PATH)
                    .query("from_broadcaster_id", broadcaster_id)
                    .query("to_broadcaster_id", target_id.as_str())
                    .query("moderator_id", actor.user_id.as_str());
                transport
                    .execute(request)
                    .await
                    .map_err(moderation_failure)?;
                self.cadence
                    .record_shoutout(&[requested_key.as_str(), id_key.as_str()]);
            }
            ChatDelivery::WhisperToLogin { login, text } => {
                send_whisper(transport, &actor.user_id, login, text)
                    .await
                    .map_err(SendFailure::Whisper)?;
            }
            ChatDelivery::WhisperToTarget { recipient, text } => {
                send_whisper_to(transport, &actor.user_id, recipient, text)
                    .await
                    .map_err(SendFailure::Whisper)?;
            }
        }
        Ok(actor.acting_sender())
    }
}

fn target_key(target: UserTarget<'_>) -> String {
    match target {
        UserTarget::Id(id) => format!("id:{id}"),
        UserTarget::Login(login) => format!("login:{}", login.to_lowercase()),
    }
}

impl ChatSenders {
    fn refuse_during_shoutout_cooldown(&self, target_keys: &[&str]) -> Result<(), SendFailure> {
        match self.cadence.shoutout_cooldown(target_keys) {
            Some(remaining) => Err(SendFailure::Refused(format!(
                "shoutout cooldown: try again in {}s",
                remaining.as_secs().max(1)
            ))),
            None => Ok(()),
        }
    }
}

fn moderation_failure(error: HelixError) -> SendFailure {
    match error {
        HelixError::ReauthRequired => SendFailure::Helix(HelixError::ReauthRequired),
        HelixError::RateLimited => SendFailure::Refused(
            "Twitch rate limit or cooldown reached; try again later".to_owned(),
        ),
        HelixError::Http { status, body } if status == forge_net::status::FORBIDDEN => {
            SendFailure::PermissionDenied(format!("HTTP {status}: {}", refusal_text(&body)))
        }
        HelixError::Http { status, body } => {
            SendFailure::Refused(format!("HTTP {status}: {}", refusal_text(&body)))
        }
        other => SendFailure::Credentials(other.to_string()),
    }
}

fn announce_fallback(
    events: &dyn EventPublisher,
    reason: BotUnavailable,
    delivery: &ChatDelivery<'_>,
) {
    warn!(
        reason = %reason,
        operation = delivery.operation(),
        "bot account unavailable; sending as the streamer"
    );
    SenderFallback {
        level: FallbackLevel::Warning,
        operation: delivery.operation().to_owned(),
        reason: reason.to_string(),
    }
    .publish(events, EventSource::Twitch, None);
}

pub(crate) type StepResult = (SubActionOutcome, BTreeMap<String, String>);

pub(crate) fn step_result(result: Result<ActingSender, SendFailure>) -> StepResult {
    match result {
        Ok(acting) => {
            let produced = serde_json::to_string(&acting)
                .map(|json| BTreeMap::from([(SENT_AS_FIELD.to_owned(), json)]))
                .unwrap_or_default();
            (SubActionOutcome::Success, produced)
        }
        Err(failure) => (
            SubActionOutcome::Failed(failure.to_string()),
            BTreeMap::new(),
        ),
    }
}

pub(crate) fn failed_step(reason: &str) -> StepResult {
    (SubActionOutcome::Failed(reason.to_owned()), BTreeMap::new())
}

#[cfg(test)]
mod tests {
    use forge_registry::runner::SubActionConfig;
    use forge_types::Variant;
    use serde_json::{Value, json};
    use wiremock::ResponseTemplate;

    use super::*;
    use crate::account::user_target::UserTarget;
    use crate::auth::TWITCH_BOT_SCOPES;
    use crate::bot_account::BotStatus;
    use crate::bot_account::test_support::{
        BOT_ACCESS, BOT_LOGIN, BOT_USER_ID, BotHarness, CLIENT_ID,
    };
    use crate::credentials::TWITCH_BOT_CREDENTIAL_ID;
    use crate::sub_actions::test_support::{MockTransport, SELF_USER_ID, users_fixture};

    const CHAT_PATH: &str = "/helix/chat/messages";
    const WHISPER_PATH: &str = "/helix/whispers";

    struct Rig {
        harness: BotHarness,
        streamer: Arc<MockTransport>,
        senders: ChatSenders,
    }

    impl Rig {
        fn over(harness: BotHarness, streamer_responses: Vec<Result<Value, HelixError>>) -> Self {
            let streamer = Arc::new(MockTransport::returning_sequence(streamer_responses));
            let senders = ChatSenders::new(
                Arc::clone(&streamer) as Arc<dyn HelixTransport>,
                Arc::new(SelfIdentity::new(Arc::clone(&harness.repo) as _)),
            )
            .with_bot(Some(Arc::clone(&harness.bot)));
            Self {
                harness,
                streamer,
                senders,
            }
        }

        async fn send(
            &self,
            delivery: ChatDelivery<'_>,
            preference: SenderPreference,
        ) -> Result<ActingSender, SendFailure> {
            self.senders
                .deliver(delivery, preference, self.harness.bus.as_ref())
                .await
        }

        async fn bot_bodies(&self, route: &str) -> Vec<Value> {
            self.harness
                .requests_to(route)
                .await
                .iter()
                .map(|request| serde_json::from_slice(&request.body).unwrap_or(Value::Null))
                .collect()
        }

        fn fallbacks(&self) -> Vec<Value> {
            self.harness
                .bus
                .of_kind(crate::vocabulary::event_kinds::SENDER_FALLBACK)
                .into_iter()
                .map(|event| event.payload)
                .collect()
        }
    }

    fn sent_body() -> Value {
        json!({"data": [{"message_id": "m-1", "is_sent": true}]})
    }

    fn chat_accepted() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(sent_body())
    }

    fn rotated_tokens() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "rotated_bot_access",
            "refresh_token": "rotated_bot_refresh",
            "expires_in": 14400,
        }))
    }

    const MESSAGE: ChatDelivery<'static> = ChatDelivery::Message { text: "hello" };
    const REPLY: ChatDelivery<'static> = ChatDelivery::Reply {
        text: "hello",
        parent_message_id: "parent-1",
    };
    const WHISPER: ChatDelivery<'static> = ChatDelivery::WhisperToLogin {
        login: "viewer",
        text: "psst",
    };

    fn as_streamer() -> ActingSender {
        ActingSender {
            role: SenderRole::Streamer,
            login: "streamer".to_owned(),
        }
    }

    fn as_bot() -> ActingSender {
        ActingSender {
            role: SenderRole::Bot,
            login: BOT_LOGIN.to_owned(),
        }
    }

    async fn connected_rig() -> Rig {
        let rig = Rig::over(BotHarness::connected().await, vec![Ok(sent_body())]);
        rig.harness.mount("POST", CHAT_PATH, chat_accepted()).await;
        rig
    }

    #[tokio::test]
    async fn default_sends_as_the_connected_bot_with_the_bot_token() {
        let rig = connected_rig().await;

        let acting = rig.send(MESSAGE, SenderPreference::Default).await.unwrap();

        assert_eq!(acting, as_bot());
        let request = &rig.harness.requests_to(CHAT_PATH).await[0];
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            format!("Bearer {BOT_ACCESS}").as_str()
        );
        assert_eq!(rig.bot_bodies(CHAT_PATH).await[0]["sender_id"], BOT_USER_ID);
        assert_eq!(rig.streamer.call_count(), 0);
        assert!(rig.fallbacks().is_empty());
    }

    #[tokio::test]
    async fn streamer_preference_sends_as_the_broadcaster_even_with_a_bot_connected() {
        let rig = connected_rig().await;

        let acting = rig.send(MESSAGE, SenderPreference::Streamer).await.unwrap();

        assert_eq!(acting, as_streamer());
        assert_eq!(
            rig.streamer.last_request().body.unwrap()["sender_id"],
            SELF_USER_ID
        );
        assert!(rig.harness.requests_to(CHAT_PATH).await.is_empty());
    }

    #[tokio::test]
    async fn every_preference_and_chat_operation_targets_the_broadcaster_channel() {
        for preference in SenderPreference::ALL {
            for delivery in [MESSAGE, REPLY] {
                let rig = connected_rig().await;

                rig.send(delivery, preference).await.unwrap();

                let mut bodies = rig.bot_bodies(CHAT_PATH).await;
                bodies.extend(
                    (0..rig.streamer.call_count())
                        .map(|index| rig.streamer.request(index).body.unwrap()),
                );
                assert_eq!(bodies.len(), 1, "{preference:?} {}", delivery.operation());
                assert_eq!(
                    bodies[0]["broadcaster_id"],
                    SELF_USER_ID,
                    "{preference:?} {}",
                    delivery.operation()
                );
            }
        }
    }

    #[tokio::test]
    async fn default_falls_back_to_the_streamer_whenever_the_bot_is_unavailable() {
        enum Setup {
            NoBotRow,
            UnreadableBotRow,
            ReauthRequired,
            RejectedAfterRefresh,
            RefreshRejected,
        }
        let reauth = Some(BotUnavailable::ReauthRequired);
        for (label, setup, warned) in [
            ("no bot row", Setup::NoBotRow, None),
            (
                "unreadable bot row",
                Setup::UnreadableBotRow,
                Some(BotUnavailable::StorageFailure),
            ),
            ("bot needs reauth", Setup::ReauthRequired, reauth),
            (
                "token rejected after refresh",
                Setup::RejectedAfterRefresh,
                reauth,
            ),
            ("refresh rejected", Setup::RefreshRejected, reauth),
        ] {
            let harness = match setup {
                Setup::NoBotRow | Setup::UnreadableBotRow => BotHarness::without_bot_row().await,
                _ => BotHarness::connected().await,
            };
            let rig = Rig::over(harness, vec![Ok(sent_body())]);
            match setup {
                Setup::NoBotRow => {}
                Setup::UnreadableBotRow => rig
                    .harness
                    .repo
                    .put(TWITCH_BOT_CREDENTIAL_ID, "{not json".to_owned()),
                Setup::ReauthRequired => rig.harness.bot.mark_reauth_required(),
                Setup::RejectedAfterRefresh => {
                    rig.harness
                        .mount("POST", CHAT_PATH, ResponseTemplate::new(401))
                        .await;
                    rig.harness.mount("POST", "/token", rotated_tokens()).await;
                }
                Setup::RefreshRejected => {
                    rig.harness
                        .mount("POST", CHAT_PATH, ResponseTemplate::new(401))
                        .await;
                    rig.harness
                        .mount("POST", "/token", ResponseTemplate::new(400))
                        .await;
                }
            }

            let acting = rig.send(MESSAGE, SenderPreference::Default).await;

            assert_eq!(acting.unwrap(), as_streamer(), "{label}");
            assert_eq!(rig.streamer.call_count(), 1, "{label}");
            let reasons: Vec<Value> = rig
                .fallbacks()
                .iter()
                .map(|payload| payload["reason"].clone())
                .collect();
            let expected: Vec<Value> = warned
                .iter()
                .map(|reason| Value::String(reason.to_string()))
                .collect();
            assert_eq!(reasons, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn fallback_publishes_a_warning_naming_the_operation_and_reason() {
        let rig = Rig::over(BotHarness::connected().await, vec![Ok(sent_body())]);
        rig.harness.bot.mark_reauth_required();

        rig.send(REPLY, SenderPreference::Default).await.unwrap();

        assert_eq!(
            rig.fallbacks(),
            vec![json!({
                "level": "warning",
                "operation": "reply",
                "reason": BotUnavailable::ReauthRequired.to_string(),
            })]
        );
    }

    #[tokio::test]
    async fn a_rejected_bot_token_is_not_retried_even_before_the_status_was_synced() {
        let rig = Rig::over(
            BotHarness::with_bot_row().await,
            vec![Ok(sent_body()), Ok(sent_body())],
        );
        rig.harness
            .mount("POST", CHAT_PATH, ResponseTemplate::new(401))
            .await;
        rig.harness.mount("POST", "/token", rotated_tokens()).await;

        rig.send(MESSAGE, SenderPreference::Default).await.unwrap();
        let chat_attempts = rig.harness.requests_to(CHAT_PATH).await.len();
        let refreshes = rig.harness.requests_to("/token").await.len();
        let second = rig.send(MESSAGE, SenderPreference::Default).await;

        assert_eq!(second.unwrap(), as_streamer());
        assert_eq!(
            rig.harness.requests_to(CHAT_PATH).await.len(),
            chat_attempts
        );
        assert_eq!(
            rig.harness.requests_to("/token").await.len(),
            refreshes,
            "a rejected bot token must not be refreshed again on the next send"
        );
    }

    #[tokio::test]
    async fn a_bot_token_rejected_after_refresh_marks_the_bot_reauth_and_stops_using_it() {
        let rig = Rig::over(
            BotHarness::connected().await,
            vec![Ok(sent_body()), Ok(sent_body())],
        );
        rig.harness
            .mount("POST", CHAT_PATH, ResponseTemplate::new(401))
            .await;
        rig.harness.mount("POST", "/token", rotated_tokens()).await;

        rig.send(MESSAGE, SenderPreference::Default).await.unwrap();
        let bot_attempts = rig.harness.requests_to(CHAT_PATH).await.len();
        rig.send(MESSAGE, SenderPreference::Default).await.unwrap();

        assert!(matches!(
            rig.harness.bot.status(),
            BotStatus::ReauthRequired { .. }
        ));
        assert_eq!(
            rig.harness.requests_to(CHAT_PATH).await.len(),
            bot_attempts,
            "a bot marked reauth-required must not be tried again"
        );
    }

    #[tokio::test]
    async fn bot_only_fails_visibly_and_sends_nothing_when_the_bot_is_unavailable() {
        for expected in [
            BotUnavailable::NotConnected,
            BotUnavailable::ReauthRequired,
            BotUnavailable::StorageFailure,
        ] {
            let harness = match expected {
                BotUnavailable::ReauthRequired => {
                    let harness = BotHarness::connected().await;
                    harness.bot.mark_reauth_required();
                    harness
                }
                _ => BotHarness::without_bot_row().await,
            };
            if expected == BotUnavailable::StorageFailure {
                harness
                    .repo
                    .put(TWITCH_BOT_CREDENTIAL_ID, "{not json".to_owned());
            }
            let rig = Rig::over(harness, vec![Ok(sent_body())]);

            let failure = rig
                .send(MESSAGE, SenderPreference::BotOnly)
                .await
                .unwrap_err();

            assert!(
                matches!(failure, SendFailure::BotNotConnected(reason) if reason == expected),
                "{expected:?}: {failure:?}"
            );
            assert_eq!(rig.streamer.call_count(), 0, "{expected:?}");
            assert!(rig.fallbacks().is_empty(), "{expected:?}");
        }
    }

    #[tokio::test]
    async fn bot_refusals_never_fall_back_or_resend() {
        type Expectation = fn(&SendFailure) -> bool;
        type Refusal = (&'static str, fn() -> ResponseTemplate, Expectation);
        let refusals: [Refusal; 3] = [
            (
                "is_sent=false",
                || {
                    ResponseTemplate::new(200).set_body_json(json!({"data": [{
                        "message_id": "",
                        "is_sent": false,
                        "drop_reason": {"code": "user_banned", "message": "You are banned."}
                    }]}))
                },
                |f| matches!(f, SendFailure::Chat(ChatSendError::Dropped { code, .. }) if code == "user_banned"),
            ),
            (
                "403",
                || {
                    ResponseTemplate::new(403).set_body_json(json!({
                        "error": "Forbidden",
                        "status": 403,
                        "message": "The sender is banned from this channel."
                    }))
                },
                |f| matches!(f, SendFailure::Chat(ChatSendError::Forbidden(m)) if m == "The sender is banned from this channel."),
            ),
            (
                "429",
                || ResponseTemplate::new(429),
                |f| matches!(f, SendFailure::Chat(ChatSendError::RateLimited)),
            ),
        ];

        for preference in [SenderPreference::Default, SenderPreference::BotOnly] {
            for (label, response, expected) in refusals {
                let rig = Rig::over(BotHarness::connected().await, vec![Ok(sent_body())]);
                rig.harness.mount("POST", CHAT_PATH, response()).await;

                let failure = rig.send(MESSAGE, preference).await.unwrap_err();

                assert!(expected(&failure), "{preference:?} {label}: {failure:?}");
                assert_eq!(rig.streamer.call_count(), 0, "{preference:?} {label}");
                assert_eq!(
                    rig.harness.requests_to(CHAT_PATH).await.len(),
                    1,
                    "{preference:?} {label} must not be resent"
                );
                assert_eq!(
                    rig.harness.bot.status(),
                    BotStatus::Connected {
                        login: BOT_LOGIN.to_owned()
                    }
                );
                assert!(rig.fallbacks().is_empty(), "{preference:?} {label}");
            }
        }
    }

    async fn whisper_rig_rejecting_the_bot(validate_status: u16) -> Rig {
        let rig = Rig::over(
            BotHarness::connected().await,
            vec![users_fixture("555"), Ok(Value::Null)],
        );
        rig.harness
            .mount(
                "GET",
                "/helix/users",
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "555", "login": "viewer"}]})),
            )
            .await;
        rig.harness
            .mount("POST", WHISPER_PATH, ResponseTemplate::new(401))
            .await;
        rig.harness.mount("POST", "/token", rotated_tokens()).await;
        rig.harness
            .mount_validate(validate_status, TWITCH_BOT_SCOPES, CLIENT_ID)
            .await;
        rig
    }

    #[tokio::test]
    async fn whisper_401_without_a_confirmed_invalid_token_is_a_phone_refusal_without_fallback() {
        for validate_status in [200, 503] {
            let rig = whisper_rig_rejecting_the_bot(validate_status).await;

            let failure = rig
                .send(WHISPER, SenderPreference::Default)
                .await
                .unwrap_err();

            assert!(
                matches!(failure, SendFailure::BotPhoneUnverified),
                "validate {validate_status}: {failure:?}"
            );
            assert_eq!(rig.streamer.call_count(), 0, "validate {validate_status}");
            assert!(rig.fallbacks().is_empty(), "validate {validate_status}");
            assert!(
                matches!(rig.harness.bot.status(), BotStatus::Connected { .. }),
                "validate {validate_status}"
            );
        }
    }

    #[tokio::test]
    async fn whisper_401_with_a_failing_bot_validation_falls_back_as_an_unavailable_bot() {
        let rig = whisper_rig_rejecting_the_bot(401).await;

        let acting = rig.send(WHISPER, SenderPreference::Default).await.unwrap();

        assert_eq!(acting, as_streamer());
        assert_eq!(rig.streamer.last_request().path, WHISPER_PATH);
        assert!(matches!(
            rig.harness.bot.status(),
            BotStatus::ReauthRequired { .. }
        ));
    }

    #[test]
    fn configured_sender_reads_absent_as_default_and_rejects_unknown_values() {
        let with = |value: Option<Variant>| -> SubActionConfig {
            value
                .map(|value| BTreeMap::from([(SENDER_FIELD.to_owned(), value)]))
                .unwrap_or_default()
        };
        let accepted = [
            (None, SenderPreference::Default),
            (Some("default"), SenderPreference::Default),
            (Some("streamer"), SenderPreference::Streamer),
            (Some("bot"), SenderPreference::BotOnly),
        ];
        for (raw, expected) in accepted {
            let config = with(raw.map(|raw| Variant::String(raw.to_owned())));
            assert_eq!(configured_sender(&config).unwrap(), expected, "{raw:?}");
        }
        for rejected in [
            Variant::String("Bot".to_owned()),
            Variant::String("bot_only".to_owned()),
            Variant::String(String::new()),
            Variant::Int(1),
        ] {
            let err = configured_sender(&with(Some(rejected.clone()))).unwrap_err();
            assert!(
                matches!(err, RegistryError::InvalidConfig(_)),
                "{rejected:?} -> {err:?}"
            );
        }
    }

    const ANNOUNCE_PATH: &str = "/helix/chat/announcements";
    const SHOUTOUT_PATH: &str = "/helix/chat/shoutouts";
    const ANNOUNCE: ChatDelivery<'static> = ChatDelivery::Announcement {
        text: "hello",
        color: "primary",
    };
    const SHOUTOUT: ChatDelivery<'static> = ChatDelivery::Shoutout {
        target: UserTarget::Id("555"),
    };

    fn path_of(delivery: &ChatDelivery<'_>) -> &'static str {
        match delivery {
            ChatDelivery::Announcement { .. } => ANNOUNCE_PATH,
            _ => SHOUTOUT_PATH,
        }
    }

    fn channel_key(delivery: &ChatDelivery<'_>) -> &'static str {
        match delivery {
            ChatDelivery::Announcement { .. } => "broadcaster_id",
            _ => "from_broadcaster_id",
        }
    }

    async fn with_moderator_lookup(rig: &Rig, listed: bool) {
        let data = if listed {
            json!({"data": [{"user_id": BOT_USER_ID}]})
        } else {
            json!({"data": []})
        };
        rig.harness
            .bot
            .attach_broadcaster_transport(Arc::new(MockTransport::returning_sequence(vec![
                Ok(data),
                Ok(json!({"data": []})),
            ])));
        rig.harness.bot.refresh_moderator_status().await;
    }

    #[tokio::test]
    async fn moderator_actions_act_as_the_resolved_sender_in_the_broadcaster_channel() {
        for delivery in [ANNOUNCE, SHOUTOUT] {
            for (preference, actor_id) in [
                (SenderPreference::Default, BOT_USER_ID),
                (SenderPreference::Streamer, SELF_USER_ID),
            ] {
                let rig = Rig::over(BotHarness::connected().await, vec![Ok(Value::Null)]);
                rig.harness
                    .mount("POST", path_of(&delivery), ResponseTemplate::new(204))
                    .await;

                rig.send(delivery, preference).await.unwrap();

                let query: Vec<(String, String)> = if actor_id == BOT_USER_ID {
                    rig.harness.requests_to(path_of(&delivery)).await[0]
                        .url
                        .query_pairs()
                        .into_owned()
                        .collect()
                } else {
                    rig.streamer.last_request().query
                };
                let label = format!("{preference:?} {}", delivery.operation());
                assert!(
                    query.contains(&(channel_key(&delivery).to_owned(), SELF_USER_ID.to_owned())),
                    "{label}: {query:?}"
                );
                assert!(
                    query.contains(&("moderator_id".to_owned(), actor_id.to_owned())),
                    "{label}: {query:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_bot_known_not_to_be_a_moderator_fails_moderator_actions_without_any_request() {
        for preference in [SenderPreference::Default, SenderPreference::BotOnly] {
            for delivery in [ANNOUNCE, SHOUTOUT] {
                let rig = Rig::over(BotHarness::connected().await, vec![Ok(Value::Null)]);
                with_moderator_lookup(&rig, false).await;

                let failure = rig.send(delivery, preference).await.unwrap_err();

                let label = format!("{preference:?} {}", delivery.operation());
                assert!(
                    matches!(failure, SendFailure::BotNotModerator),
                    "{label}: {failure:?}"
                );
                assert!(
                    rig.harness.requests_to(path_of(&delivery)).await.is_empty(),
                    "{label}"
                );
                assert_eq!(rig.streamer.call_count(), 0, "{label}");
                assert!(rig.fallbacks().is_empty(), "{label}");
            }
        }
    }

    #[tokio::test]
    async fn a_bot_that_is_not_a_moderator_still_sends_plain_chat() {
        let rig = connected_rig().await;
        with_moderator_lookup(&rig, false).await;

        let acting = rig.send(MESSAGE, SenderPreference::Default).await.unwrap();

        assert_eq!(acting, as_bot());
    }

    #[tokio::test]
    async fn a_403_on_a_bot_moderator_action_rechecks_moderator_status_and_is_a_refusal() {
        let rig = Rig::over(BotHarness::connected().await, vec![Ok(Value::Null)]);
        with_moderator_lookup(&rig, true).await;
        rig.harness
            .mount(
                "POST",
                ANNOUNCE_PATH,
                ResponseTemplate::new(403)
                    .set_body_json(json!({"status": 403, "message": "not a moderator"})),
            )
            .await;

        let failure = rig
            .send(ANNOUNCE, SenderPreference::Default)
            .await
            .unwrap_err();

        assert!(
            matches!(&failure, SendFailure::PermissionDenied(m) if m.contains("not a moderator")),
            "{failure:?}"
        );
        assert_eq!(
            rig.harness.bot.moderator_status(),
            crate::bot_account::BotModerator::NotModerator
        );
        assert_eq!(rig.streamer.call_count(), 0);
    }

    #[tokio::test]
    async fn moderator_action_refusals_never_fall_back_or_resend() {
        for delivery in [ANNOUNCE, SHOUTOUT] {
            for (status, message) in [(400, "channel is offline"), (429, "cooldown")] {
                let rig = Rig::over(BotHarness::connected().await, vec![Ok(Value::Null)]);
                rig.harness
                    .mount(
                        "POST",
                        path_of(&delivery),
                        ResponseTemplate::new(status)
                            .set_body_json(json!({"status": status, "message": message})),
                    )
                    .await;

                let failure = rig
                    .send(delivery, SenderPreference::Default)
                    .await
                    .unwrap_err();

                let label = format!("{} {status}", delivery.operation());
                assert!(
                    matches!(failure, SendFailure::Refused(_)),
                    "{label}: {failure:?}"
                );
                assert_eq!(rig.streamer.call_count(), 0, "{label}");
                assert_eq!(
                    rig.harness.requests_to(path_of(&delivery)).await.len(),
                    1,
                    "{label}"
                );
                assert!(rig.fallbacks().is_empty(), "{label}");
            }
        }
    }

    #[tokio::test]
    async fn a_rejected_bot_token_on_a_moderator_action_falls_back_to_the_streamer() {
        let rig = Rig::over(BotHarness::connected().await, vec![Ok(Value::Null)]);
        rig.harness
            .mount("POST", ANNOUNCE_PATH, ResponseTemplate::new(401))
            .await;
        rig.harness
            .mount("POST", "/token", ResponseTemplate::new(400))
            .await;

        let acting = rig.send(ANNOUNCE, SenderPreference::Default).await.unwrap();

        assert_eq!(acting, as_streamer());
        assert!(
            rig.streamer
                .last_request()
                .query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned()))
        );
    }

    fn cooldown_refusal(result: &Result<ActingSender, SendFailure>) -> bool {
        matches!(result, Err(SendFailure::Refused(m)) if m.starts_with("shoutout cooldown"))
    }

    #[tokio::test]
    async fn a_shoutout_in_cooldown_is_refused_for_every_sender_without_any_helix_call() {
        let rig = Rig::over(
            BotHarness::connected().await,
            vec![users_fixture("555"), Ok(Value::Null)],
        );
        tokio::time::pause();
        let target = ChatDelivery::Shoutout {
            target: UserTarget::Login("Target"),
        };
        rig.send(target, SenderPreference::Streamer).await.unwrap();

        let as_bot = rig
            .send(
                ChatDelivery::Shoutout {
                    target: UserTarget::Login("target"),
                },
                SenderPreference::BotOnly,
            )
            .await;
        let other_target = rig
            .send(
                ChatDelivery::Shoutout {
                    target: UserTarget::Login("other"),
                },
                SenderPreference::Streamer,
            )
            .await;

        assert!(cooldown_refusal(&as_bot), "{as_bot:?}");
        assert!(cooldown_refusal(&other_target), "{other_target:?}");
        assert_eq!(rig.streamer.call_count(), 2);
        assert!(rig.harness.requests_to(SHOUTOUT_PATH).await.is_empty());
        assert!(rig.harness.requests_to("/helix/users").await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_target_shouted_out_by_login_stays_blocked_by_id_after_the_channel_window() {
        let rig = Rig::over(
            BotHarness::without_bot_row().await,
            vec![
                users_fixture("555"),
                Ok(Value::Null),
                users_fixture("777"),
                Ok(Value::Null),
            ],
        );
        rig.send(
            ChatDelivery::Shoutout {
                target: UserTarget::Login("target"),
            },
            SenderPreference::Streamer,
        )
        .await
        .unwrap();
        tokio::time::advance(
            crate::cadence::SHOUTOUT_CHANNEL_INTERVAL + std::time::Duration::from_secs(1),
        )
        .await;

        let same_by_id = rig.send(SHOUTOUT, SenderPreference::Streamer).await;
        let calls_after_blocked = rig.streamer.call_count();
        let other = rig
            .send(
                ChatDelivery::Shoutout {
                    target: UserTarget::Login("other"),
                },
                SenderPreference::Streamer,
            )
            .await;

        assert!(cooldown_refusal(&same_by_id), "{same_by_id:?}");
        assert_eq!(calls_after_blocked, 2);
        assert_eq!(other.unwrap(), as_streamer());
        assert_eq!(rig.streamer.call_count(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_shoutout_leaves_no_cooldown_behind() {
        let rig = Rig::over(
            BotHarness::without_bot_row().await,
            vec![
                Err(HelixError::Http {
                    status: 400,
                    body: json!({"message": "channel is offline"}).to_string(),
                }),
                Ok(Value::Null),
            ],
        );
        let refused = rig.send(SHOUTOUT, SenderPreference::Streamer).await;

        let retried = rig.send(SHOUTOUT, SenderPreference::Streamer).await;

        assert!(matches!(refused, Err(SendFailure::Refused(_))));
        assert_eq!(retried.unwrap(), as_streamer());
        assert_eq!(rig.streamer.call_count(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn back_to_back_announcements_go_out_exactly_one_interval_apart() {
        let rig = Rig::over(
            BotHarness::without_bot_row().await,
            vec![Ok(Value::Null), Ok(Value::Null)],
        );
        let started = tokio::time::Instant::now();

        rig.send(ANNOUNCE, SenderPreference::Streamer)
            .await
            .unwrap();
        let after_first = started.elapsed();
        rig.send(ANNOUNCE, SenderPreference::Streamer)
            .await
            .unwrap();

        assert_eq!(after_first, std::time::Duration::ZERO);
        assert_eq!(started.elapsed(), crate::cadence::ANNOUNCEMENT_INTERVAL);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_announcement_does_not_delay_the_next_one() {
        let rig = Rig::over(
            BotHarness::without_bot_row().await,
            vec![
                Err(HelixError::Http {
                    status: 400,
                    body: json!({"message": "channel is offline"}).to_string(),
                }),
                Ok(Value::Null),
            ],
        );
        let started = tokio::time::Instant::now();

        let failed = rig.send(ANNOUNCE, SenderPreference::Streamer).await;
        rig.send(ANNOUNCE, SenderPreference::Streamer)
            .await
            .unwrap();

        assert!(matches!(failed, Err(SendFailure::Refused(_))));
        assert_eq!(started.elapsed(), std::time::Duration::ZERO);
    }

    #[tokio::test]
    async fn a_whisper_the_recipient_refuses_never_falls_back() {
        let rig = Rig::over(BotHarness::connected().await, vec![Ok(Value::Null)]);
        rig.harness
            .mount(
                "GET",
                "/helix/users",
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "555", "login": "viewer"}]})),
            )
            .await;
        rig.harness
            .mount(
                "POST",
                WHISPER_PATH,
                ResponseTemplate::new(400).set_body_json(json!({
                    "status": 400,
                    "message": "The recipient doesn't allow whispers from strangers"
                })),
            )
            .await;

        let failure = rig
            .send(WHISPER, SenderPreference::Default)
            .await
            .unwrap_err();

        assert!(
            matches!(
                failure,
                SendFailure::Whisper(WhisperError::RecipientRejectsWhispers)
            ),
            "{failure:?}"
        );
        assert_eq!(rig.streamer.call_count(), 0);
    }
}
