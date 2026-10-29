use super::event_kinds;

pub const CHAT_MESSAGE: &str = event_kinds::CHAT_MESSAGE;
pub const CHAT_COMMAND: &str = "youtube.chat.command";
pub const CHAT_MESSAGE_DELETED: &str = event_kinds::CHAT_MESSAGE_DELETED;
pub const CHAT_SUPER_CHAT: &str = event_kinds::CHAT_SUPER_CHAT;
pub const CHAT_SUPER_STICKER: &str = event_kinds::CHAT_SUPER_STICKER;
pub const CHANNEL_MEMBER: &str = event_kinds::CHANNEL_MEMBER;
pub const CHANNEL_MEMBER_MILESTONE: &str = event_kinds::CHANNEL_MEMBER_MILESTONE;
pub const CHANNEL_MEMBER_GIFT: &str = event_kinds::CHANNEL_MEMBER_GIFT;
pub const CHANNEL_MEMBER_GIFT_RECEIVED: &str = event_kinds::CHANNEL_MEMBER_GIFT_RECEIVED;
pub const CHANNEL_USER_BANNED: &str = event_kinds::CHANNEL_USER_BANNED;
pub const STREAM_ONLINE: &str = event_kinds::STREAM_ONLINE;
pub const STREAM_OFFLINE: &str = event_kinds::STREAM_OFFLINE;
pub const STREAM_TITLE_CHANGED: &str = event_kinds::STREAM_TITLE_CHANGED;
