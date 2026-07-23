pub(crate) mod entity {
    pub(crate) const CHANNEL_ID: &str = "channel_id";
    pub(crate) const DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod chat {
    pub(crate) const MESSAGE_TEXT: &str = "message_text";
    pub(crate) const AUTHOR: &str = "author";
    pub(crate) const BROADCASTER_CHANNEL_ID: &str = "broadcaster_channel_id";
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
    pub(crate) const COUNT: &str = "count";
    pub(crate) const LEVEL_NAME: &str = "level_name";
    pub(crate) const GIFTER: &str = "gifter";
    pub(crate) const RECIPIENT: &str = "recipient";
}

pub(crate) mod ban {
    pub(crate) const TARGET_USER: &str = "target_user";
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const TYPE: &str = "type";
    pub(crate) const DURATION_SECS: &str = "duration_secs";
}

pub(crate) mod chat_mod {
    pub(crate) const MESSAGE_ID: &str = "message_id";
}

pub(crate) mod stream {
    pub(crate) const TITLE: &str = "title";
    pub(crate) const OLD: &str = "old";
    pub(crate) const NEW: &str = "new";
    pub(crate) const BROADCAST_ID: &str = "broadcast_id";
    pub(crate) const BROADCAST_TITLE: &str = "broadcast_title";
}
