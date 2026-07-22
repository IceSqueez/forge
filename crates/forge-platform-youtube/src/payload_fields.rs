pub(crate) mod chat {
    pub(crate) const MESSAGE_TEXT: &str = "message_text";
    pub(crate) const USER_DISPLAY_NAME: &str = "user_display_name";
    pub(crate) const CHANNEL_ID: &str = "channel_id";
    pub(crate) const COMMAND_NAME: &str = "command_name";
    pub(crate) const ARGS: &str = "args";
}

pub(crate) mod support {
    pub(crate) const AMOUNT_MICROS: &str = "amount_micros";
    pub(crate) const CURRENCY: &str = "currency";
    pub(crate) const STICKER_ID: &str = "sticker_id";
}

pub(crate) mod member {
    pub(crate) const MEMBER_LEVEL_NAME: &str = "member_level_name";
    pub(crate) const MEMBER_MONTH: &str = "member_month";
}

pub(crate) mod gift {
    pub(crate) const COUNT: &str = "gift.count";
    pub(crate) const LEVEL_NAME: &str = "gift.level_name";
    pub(crate) const GIFTER_CHANNEL_ID: &str = "gifter.channel_id";
    pub(crate) const GIFTER_DISPLAY_NAME: &str = "gifter.display_name";
    pub(crate) const RECIPIENT_CHANNEL_ID: &str = "recipient.channel_id";
    pub(crate) const RECIPIENT_DISPLAY_NAME: &str = "recipient.display_name";
}

pub(crate) mod ban {
    pub(crate) const TARGET_DISPLAY_NAME: &str = "ban.target.display_name";
    pub(crate) const TARGET_CHANNEL_ID: &str = "ban.target.channel_id";
    pub(crate) const MODERATOR_CHANNEL_ID: &str = "ban.moderator.channel_id";
    pub(crate) const TYPE: &str = "ban.type";
    pub(crate) const DURATION_SECONDS: &str = "ban.duration_seconds";
}

pub(crate) mod chat_mod {
    pub(crate) const MESSAGE_ID: &str = "chat.message_id";
    pub(crate) const TARGET_USER_CHANNEL_ID: &str = "chat.target_user.channel_id";
    pub(crate) const MODERATOR_CHANNEL_ID: &str = "chat.moderator.channel_id";
}

pub(crate) mod stream {
    pub(crate) const TITLE_OLD: &str = "stream.title_old";
    pub(crate) const TITLE_NEW: &str = "stream.title_new";
    pub(crate) const BROADCAST_ID: &str = "broadcast_id";
    pub(crate) const BROADCAST_TITLE: &str = "broadcast_title";
}
