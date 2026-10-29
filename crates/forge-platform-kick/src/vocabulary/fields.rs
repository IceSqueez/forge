pub(crate) mod entity {
    pub(crate) const ID: &str = "id";
    pub(crate) const USERNAME: &str = "username";
    pub(crate) const DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod chat {
    pub(crate) const MESSAGE_ID: &str = "message_id";
    pub(crate) const CONTENT: &str = "content";
    pub(crate) const REPLY_TO_MESSAGE_ID: &str = "reply_to_message_id";
    pub(crate) const SENDER: &str = "sender";
    pub(crate) const COLOR: &str = "color";
    pub(crate) const DELETED_BY: &str = "deleted_by";
}

pub(crate) mod moderation {
    pub(crate) const BANNED_USER: &str = "banned_user";
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const IS_PERMANENT: &str = "is_permanent";
    pub(crate) const DURATION_SECS: &str = "duration_secs";
    pub(crate) const REASON: &str = "reason";
}

pub(crate) mod subscription {
    pub(crate) const SUBSCRIBER: &str = "subscriber";
    pub(crate) const MONTHS: &str = "months";
    pub(crate) const TIER: &str = "tier";
}

pub(crate) mod subscription_gift {
    pub(crate) const GIFTER: &str = "gifter";
    pub(crate) const GIFTEES: &str = "giftees";
    pub(crate) const COUNT: &str = "count";
    pub(crate) const TIER: &str = "tier";
}

pub(crate) mod host {
    pub(crate) const HOST: &str = "host";
    pub(crate) const VIEWER_COUNT: &str = "viewer_count";
}

pub(crate) mod stream {
    pub(crate) const IS_LIVE: &str = "is_live";
    pub(crate) const STREAM_TITLE: &str = "stream_title";
    pub(crate) const CATEGORY: &str = "category";
    pub(crate) const CATEGORY_ID: &str = "id";
    pub(crate) const CATEGORY_NAME: &str = "name";
}

pub(crate) mod reward {
    pub(crate) const ID: &str = "id";
    pub(crate) const REWARD: &str = "reward";
    pub(crate) const REWARD_TITLE: &str = "title";
    pub(crate) const REDEEMER: &str = "redeemer";
    pub(crate) const REDEEMER_USER_ID: &str = "user_id";
    pub(crate) const REDEEMER_USERNAME: &str = "username";
    pub(crate) const USER_INPUT: &str = "user_input";
}
