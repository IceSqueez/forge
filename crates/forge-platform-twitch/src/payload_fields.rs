pub(crate) mod chat {
    pub(crate) const CHANNEL: &str = "channel";
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_ROLES: &str = "roles";
    pub(crate) const MESSAGE: &str = "message";
    pub(crate) const BADGES: &str = "badges";
    pub(crate) const COLOR: &str = "color";
    pub(crate) const CHEER: &str = "cheer";
    pub(crate) const CHEER_BITS: &str = "bits";
    pub(crate) const FROM_CHANNEL: &str = "from_channel";
    pub(crate) const FROM_CHANNEL_LOGIN: &str = "login";
    pub(crate) const FROM_CHANNEL_DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod moderation {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const MODERATOR_LOGIN: &str = "login";
    pub(crate) const MODERATOR_DISPLAY_NAME: &str = "display_name";
    pub(crate) const REASON: &str = "reason";
    pub(crate) const BANNED_AT: &str = "banned_at";
    pub(crate) const ENDS_AT: &str = "ends_at";
    pub(crate) const IS_PERMANENT: &str = "is_permanent";
}

pub(crate) mod follow {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const FOLLOWED_AT: &str = "followed_at";
}

pub(crate) mod channel_update {
    pub(crate) const CHANNEL: &str = "channel";
    pub(crate) const TITLE: &str = "title";
    pub(crate) const LANGUAGE: &str = "language";
    pub(crate) const CATEGORY_ID: &str = "category_id";
    pub(crate) const CATEGORY_NAME: &str = "category_name";
}

pub(crate) mod channel_points {
    pub(crate) const REDEMPTION: &str = "redemption";
    pub(crate) const REDEMPTION_ID: &str = "id";
    pub(crate) const REDEMPTION_STATUS: &str = "status";
    pub(crate) const USER_INPUT: &str = "user_input";
    pub(crate) const REDEEMED_AT: &str = "redeemed_at";
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const REWARD: &str = "reward";
    pub(crate) const REWARD_ID: &str = "id";
    pub(crate) const REWARD_TITLE: &str = "title";
    pub(crate) const REWARD_COST: &str = "cost";
    pub(crate) const REWARD_PROMPT: &str = "prompt";
}

pub(crate) mod reward {
    pub(crate) const REWARD: &str = "reward";
    pub(crate) const REWARD_ID: &str = "id";
    pub(crate) const REWARD_TITLE: &str = "title";
    pub(crate) const REWARD_COST: &str = "cost";
    pub(crate) const REWARD_PROMPT: &str = "prompt";
    pub(crate) const REWARD_IS_ENABLED: &str = "is_enabled";
}

pub(crate) mod raid {
    pub(crate) const DIRECTION: &str = "direction";
    pub(crate) const VIEWER_COUNT: &str = "viewer_count";
    pub(crate) const FROM_BROADCASTER: &str = "from_broadcaster";
    pub(crate) const TO_BROADCASTER: &str = "to_broadcaster";
    pub(crate) const BROADCASTER_ID: &str = "id";
    pub(crate) const BROADCASTER_LOGIN: &str = "login";
    pub(crate) const BROADCASTER_DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod support {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const TIER: &str = "tier";
    pub(crate) const IS_GIFT: &str = "is_gift";
    pub(crate) const CUMULATIVE_MONTHS: &str = "cumulative_months";
    pub(crate) const STREAK_MONTHS: &str = "streak_months";
    pub(crate) const MESSAGE: &str = "message";
    pub(crate) const SHARE_STREAK: &str = "share_streak";
    pub(crate) const GIFTER: &str = "gifter";
    pub(crate) const GIFTER_LOGIN: &str = "login";
    pub(crate) const GIFTER_ID: &str = "id";
    pub(crate) const GIFTER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const IS_ANONYMOUS: &str = "is_anonymous";
    pub(crate) const RECIPIENT: &str = "recipient";
    pub(crate) const RECIPIENT_LOGIN: &str = "login";
    pub(crate) const RECIPIENT_ID: &str = "id";
    pub(crate) const RECIPIENT_DISPLAY_NAME: &str = "display_name";
    pub(crate) const BITS: &str = "bits";
}

pub(crate) mod poll {
    pub(crate) const POLL: &str = "poll";
    pub(crate) const POLL_ID: &str = "id";
    pub(crate) const POLL_TITLE: &str = "title";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const ENDS_AT: &str = "ends_at";
    pub(crate) const STATUS: &str = "status";
    pub(crate) const ENDED_AT: &str = "ended_at";
}

pub(crate) mod prediction {
    pub(crate) const PREDICTION: &str = "prediction";
    pub(crate) const PREDICTION_ID: &str = "id";
    pub(crate) const PREDICTION_TITLE: &str = "title";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const LOCKS_AT: &str = "locks_at";
    pub(crate) const LOCKED_AT: &str = "locked_at";
    pub(crate) const WINNING_OUTCOME_ID: &str = "winning_outcome_id";
    pub(crate) const STATUS: &str = "status";
    pub(crate) const ENDED_AT: &str = "ended_at";
}

pub(crate) mod hype_train {
    pub(crate) const HYPE: &str = "hype";
    pub(crate) const HYPE_ID: &str = "id";
    pub(crate) const LEVEL: &str = "level";
    pub(crate) const GOAL: &str = "goal";
    pub(crate) const PROGRESS: &str = "progress";
    pub(crate) const TOTAL: &str = "total";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const EXPIRES_AT: &str = "expires_at";
    pub(crate) const ENDED_AT: &str = "ended_at";
    pub(crate) const COOLDOWN_ENDS_AT: &str = "cooldown_ends_at";
}

pub(crate) mod goal {
    pub(crate) const GOAL: &str = "goal";
    pub(crate) const GOAL_ID: &str = "id";
    pub(crate) const GOAL_TYPE: &str = "type";
    pub(crate) const DESCRIPTION: &str = "description";
    pub(crate) const CURRENT_AMOUNT: &str = "current_amount";
    pub(crate) const TARGET_AMOUNT: &str = "target_amount";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const IS_ACHIEVED: &str = "is_achieved";
    pub(crate) const ENDED_AT: &str = "ended_at";
}
