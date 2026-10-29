pub(crate) mod chat {
    pub(crate) const COLOR: &str = "color";
    pub(crate) const REPLY_TO_ID: &str = "reply_to_id";
    pub(crate) const COMMAND_NAME: &str = "command_name";
    pub(crate) const COMMAND_ARGS: &str = "args";
}

pub(crate) mod legacy {
    pub(crate) const SENDER_ID: &str = "sender_id";
    pub(crate) const USERNAME: &str = "username";
    pub(crate) const DISPLAY_NAME: &str = "display_name";
    pub(crate) const CONTENT: &str = "content";
    pub(crate) const BANNED_USER_ID: &str = "banned_user_id";
    pub(crate) const BANNED_USERNAME: &str = "banned_username";
    pub(crate) const HOST_USERNAME: &str = "host_username";
    pub(crate) const DELETED_BY_ID: &str = "deleted_by_id";
    pub(crate) const USER_INPUT: &str = "user_input";
    pub(crate) const MONTHS: &str = "months";
    pub(crate) const TIER: &str = "tier";
    pub(crate) const GIFTER_USERNAME: &str = "gifter_username";
    pub(crate) const COUNT: &str = "count";
}

pub(crate) mod ban {
    pub(crate) const DURATION_SECS: &str = "duration_secs";
    pub(crate) const REASON: &str = "reason";
}

pub(crate) mod livestream {
    pub(crate) const IS_LIVE: &str = "is_live";
    pub(crate) const STREAM_TITLE: &str = "stream_title";
    pub(crate) const CATEGORY_ID: &str = "category_id";
    pub(crate) const CATEGORY_NAME: &str = "category_name";
}

pub(crate) mod reward {
    pub(crate) const REDEMPTION_ID: &str = "redemption_id";
    pub(crate) const REWARD_ID: &str = "reward_id";
    pub(crate) const REWARD_TITLE: &str = "reward_title";
}

pub(crate) mod viewer {
    pub(crate) const ID: &str = "kick.viewer.id";
    pub(crate) const USERNAME: &str = "kick.viewer.username";
    pub(crate) const IS_LIVE: &str = "kick.viewer.is_live";
    pub(crate) const STREAM_TITLE: &str = "kick.viewer.stream_title";
    pub(crate) const CATEGORY_NAME: &str = "kick.viewer.category_name";
    pub(crate) const VIEWER_COUNT: &str = "kick.viewer.viewer_count";
}

pub(crate) mod stream_stats {
    pub(crate) const IS_LIVE: &str = "kick.stream.is_live";
    pub(crate) const VIEWER_COUNT: &str = "kick.stream.viewer_count";
    pub(crate) const STARTED_AT: &str = "kick.stream.started_at";
    pub(crate) const CATEGORY_ID: &str = "kick.stream.category_id";
    pub(crate) const CATEGORY_NAME: &str = "kick.stream.category_name";
    pub(crate) const STREAM_TITLE: &str = "kick.stream.stream_title";
}

pub(crate) mod category {
    pub(crate) const MATCHES: &str = "kick.category.matches";
    pub(crate) const MATCH_ID: &str = "id";
    pub(crate) const MATCH_NAME: &str = "name";
}

pub(crate) mod references {
    pub(crate) const MESSAGE_ID: &str = "%message_id%";
    pub(crate) const USER_ID: &str = "%user_id%";
    pub(crate) const REWARD_ID: &str = "%reward_id%";
    pub(crate) const REDEMPTION_ID: &str = "%redemption_id%";
    pub(crate) const REDEMPTION_IDS_HINT: &str = "%redemption_id% or id1,id2,id3 (max 25)";
}
