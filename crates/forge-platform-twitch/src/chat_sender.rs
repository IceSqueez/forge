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
