use super::event_kinds;

pub const CHAT_MESSAGE_SENT: &str = event_kinds::CHAT_MESSAGE_SENT;
pub const CHAT_COMMAND: &str = "kick.chat.command";
pub const CHAT_MESSAGE_DELETED: &str = event_kinds::CHAT_MESSAGE_DELETED;
pub const MODERATION_BANNED: &str = event_kinds::MODERATION_BANNED;
pub const CHANNEL_SUBSCRIBED: &str = event_kinds::CHANNEL_SUBSCRIBED;
pub const CHANNEL_SUBSCRIPTION_GIFTS: &str = event_kinds::CHANNEL_SUBSCRIPTION_GIFTS;
pub const CHANNEL_HOSTED: &str = event_kinds::CHANNEL_HOSTED;
pub const CHANNEL_REWARD_REDEMPTION_UPDATED: &str = event_kinds::CHANNEL_REWARD_REDEMPTION_UPDATED;
pub const LIVESTREAM_STATUS_UPDATED: &str = event_kinds::LIVESTREAM_STATUS_UPDATED;
pub const LIVESTREAM_METADATA_UPDATED: &str = event_kinds::LIVESTREAM_METADATA_UPDATED;
