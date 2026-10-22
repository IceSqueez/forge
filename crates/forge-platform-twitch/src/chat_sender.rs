use std::collections::BTreeMap;
use std::sync::Arc;

use forge_events::{Event, EventPublisher, EventSource};
use forge_platform_core::{
    ActingSender, SENDER_FIELD, SENT_AS_FIELD, SenderPreference, SenderRole,
};
use forge_registry::runner::SubActionConfig;
use forge_registry::{ConfigReason, RegistryError};
use forge_types::SubActionOutcome;
use thiserror::Error;
use tracing::warn;

use crate::bot_account::{BotIdentity, BotUnavailable, TwitchBotAccount};
use crate::chat::{
    ChatSendError, WhisperError, send_chat, send_chat_reply, send_whisper, send_whisper_to,
};
use crate::helix::{HelixError, HelixTransport};
use crate::sub_actions::identity::SelfIdentity;
use crate::sub_actions::user_target::UserTarget;

pub const SENDER_FALLBACK_KIND: &str = "chat.sender.fallback";

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
    #[error("{0}")]
    Credentials(String),
}

impl SendFailure {
    fn token_rejected(&self) -> bool {
        matches!(
            self,
            Self::Chat(ChatSendError::ReauthRequired)
                | Self::Whisper(WhisperError::Helix(HelixError::ReauthRequired))
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
            Self::WhisperToLogin { .. } | Self::WhisperToTarget { .. } => "whisper",
        }
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
}

impl ChatSenders {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
            bot: None,
        }
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
            SenderPreference::Streamer => post(&streamer, &broadcaster_id, delivery).await,
            SenderPreference::BotOnly => self.send_as_bot(&broadcaster_id, delivery).await,
            SenderPreference::Default => match self.send_as_bot(&broadcaster_id, delivery).await {
                Err(SendFailure::BotNotConnected(reason)) => {
                    if reason != BotUnavailable::NotConnected {
                        announce_fallback(events, reason, &delivery);
                    }
                    post(&streamer, &broadcaster_id, delivery).await
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
        match post(&bot, broadcaster_id, delivery).await {
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

async fn post(
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
    events.publish(Event::new(
        EventSource::Twitch,
        SENDER_FALLBACK_KIND,
        serde_json::json!({
            "level": "warning",
            "operation": delivery.operation(),
            "reason": reason.to_string(),
        }),
    ));
}

pub(crate) fn configured_sender(
    config: &SubActionConfig,
) -> Result<SenderPreference, RegistryError> {
    match config.get(SENDER_FIELD) {
        None => Ok(SenderPreference::Default),
        Some(value) => value
            .as_str()
            .and_then(SenderPreference::parse)
            .ok_or_else(|| {
                RegistryError::invalid_field(
                    SENDER_FIELD,
                    ConfigReason::choices(SenderPreference::WIRE_NAMES.iter().copied()),
                )
            }),
    }
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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_registry::runner::SubActionConfig;
    use forge_types::Variant;
    use serde_json::{Value, json};
    use wiremock::ResponseTemplate;

    use super::*;
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
                .of_kind(SENDER_FALLBACK_KIND)
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
}
