use super::eventsub_topics::eventsub_topics;

macro_rules! topic_kinds {
    ($($name:ident => $topic:literal,)*) => {
        $(pub const $name: &str = concat!("twitch.", $topic);)*
    };
}

eventsub_topics!(topic_kinds);

pub const EVENTSUB_SUBSCRIPTION_CREATED: &str = "twitch.eventsub.subscription.created";
pub const BOT_STATUS_CHANGED: &str = "twitch.bot.status";
pub const SENDER_FALLBACK: &str = "chat.sender.fallback";
pub const PLATFORM_REAUTH_REQUIRED: &str = "platform.reauth_required";

pub const DESCRIPTOR_REBUILDING_KINDS: [&str; 8] = [
    CHANNEL_POLL_BEGIN,
    CHANNEL_POLL_PROGRESS,
    CHANNEL_POLL_END,
    CHANNEL_PREDICTION_BEGIN,
    CHANNEL_PREDICTION_PROGRESS,
    CHANNEL_PREDICTION_LOCK,
    CHANNEL_PREDICTION_END,
    CHANNEL_RAID,
];
