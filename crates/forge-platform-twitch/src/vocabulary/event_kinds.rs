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
