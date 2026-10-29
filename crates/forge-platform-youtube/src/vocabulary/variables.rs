pub(crate) mod chat {
    pub(crate) const COMMAND_NAME: &str = "command_name";
    pub(crate) const COMMAND_ARGS: &str = "args";
}

pub(crate) mod sender {
    pub(crate) const DISPLAY_NAME: &str = "user_display_name";
    pub(crate) const CHANNEL_ID: &str = "channel_id";
}

pub(crate) mod support {
    pub(crate) const AMOUNT_MICROS: &str = "amount_micros";
    pub(crate) const CURRENCY: &str = "currency";
    pub(crate) const STICKER_ID: &str = "sticker_id";
}

pub(crate) mod member {
    pub(crate) const LEVEL_NAME: &str = "member_level_name";
    pub(crate) const MONTH: &str = "member_month";
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
    pub(crate) const TYPE: &str = "ban.type";
    pub(crate) const DURATION_SECONDS: &str = "ban.duration_seconds";
    pub(crate) const TARGET_DISPLAY_NAME: &str = "ban.target.display_name";
    pub(crate) const TARGET_CHANNEL_ID: &str = "ban.target.channel_id";
    pub(crate) const MODERATOR_CHANNEL_ID: &str = "ban.moderator.channel_id";
    pub(crate) const MODERATOR_DISPLAY_NAME: &str = "ban.moderator.display_name";
}

pub(crate) mod stream {
    pub(crate) const BROADCAST_ID: &str = "broadcast_id";
    pub(crate) const BROADCAST_TITLE: &str = "broadcast_title";
    pub(crate) const TITLE_OLD: &str = "stream.title_old";
    pub(crate) const TITLE_NEW: &str = "stream.title_new";
}

pub(crate) mod stream_stats {
    pub(crate) const CONCURRENT_VIEWERS: &str = "youtube.stream.concurrent_viewers";
    pub(crate) const ACTUAL_START_TIME: &str = "youtube.stream.actual_start_time";
    pub(crate) const SCHEDULED_START_TIME: &str = "youtube.stream.scheduled_start_time";
    pub(crate) const LIVE_CHAT_ID: &str = "youtube.stream.live_chat_id";
}

pub(crate) mod viewer {
    pub(crate) const CHANNEL_ID: &str = "youtube.viewer.channel_id";
    pub(crate) const TITLE: &str = "youtube.viewer.title";
    pub(crate) const SUBSCRIBER_COUNT: &str = "youtube.viewer.subscriber_count";
    pub(crate) const VIEW_COUNT: &str = "youtube.viewer.view_count";
}
