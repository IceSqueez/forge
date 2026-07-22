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

pub(crate) mod guest_star {
    pub(crate) const SESSION: &str = "session";
    pub(crate) const SESSION_ID: &str = "id";
    pub(crate) const SESSION_STARTED_AT: &str = "started_at";
    pub(crate) const SESSION_ENDED_AT: &str = "ended_at";
    pub(crate) const SETTINGS: &str = "settings";
    pub(crate) const SLOT_COUNT: &str = "slot_count";
    pub(crate) const GROUP_LAYOUT: &str = "group_layout";
    pub(crate) const IS_MODERATOR_SEND_LIVE_ENABLED: &str = "is_moderator_send_live_enabled";
    pub(crate) const IS_BROWSER_SOURCE_AUDIO_ENABLED: &str = "is_browser_source_audio_enabled";
    pub(crate) const GUEST_STAR: &str = "guest_star";
    pub(crate) const SESSION_ID_FIELD: &str = "session_id";
    pub(crate) const SLOT_ID_FIELD: &str = "slot_id";
    pub(crate) const STATE: &str = "state";
    pub(crate) const GUEST: &str = "guest";
    pub(crate) const GUEST_ID: &str = "id";
    pub(crate) const GUEST_LOGIN: &str = "login";
    pub(crate) const GUEST_DISPLAY_NAME: &str = "display_name";
    pub(crate) const HOST: &str = "host";
    pub(crate) const HOST_VIDEO_ENABLED: &str = "video_enabled";
    pub(crate) const HOST_AUDIO_ENABLED: &str = "audio_enabled";
    pub(crate) const HOST_VOLUME: &str = "volume";
    pub(crate) const SLOT: &str = "slot";
    pub(crate) const SLOT_HOST_VIDEO_ENABLED: &str = "host_video_enabled";
    pub(crate) const SLOT_HOST_AUDIO_ENABLED: &str = "host_audio_enabled";
    pub(crate) const SLOT_VOLUME: &str = "volume";
}

pub(crate) mod shared_chat {
    pub(crate) const SHARED_CHAT: &str = "shared_chat";
    pub(crate) const SESSION_ID: &str = "session_id";
    pub(crate) const HOST: &str = "host";
    pub(crate) const HOST_ID: &str = "id";
    pub(crate) const HOST_LOGIN: &str = "login";
    pub(crate) const HOST_DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod charity {
    pub(crate) const CHARITY: &str = "charity";
    pub(crate) const CHARITY_ID: &str = "id";
    pub(crate) const CHARITY_NAME: &str = "name";
    pub(crate) const DESCRIPTION: &str = "description";
    pub(crate) const WEBSITE: &str = "website";
    pub(crate) const AMOUNT_CENTS: &str = "amount_cents";
    pub(crate) const CURRENT_AMOUNT_CENTS: &str = "current_amount_cents";
    pub(crate) const TARGET_AMOUNT_CENTS: &str = "target_amount_cents";
    pub(crate) const CURRENCY_CODE: &str = "currency_code";
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod automod {
    pub(crate) const AUTOMOD: &str = "automod";
    pub(crate) const MESSAGE_ID: &str = "message_id";
    pub(crate) const CATEGORY: &str = "category";
    pub(crate) const LEVEL: &str = "level";
    pub(crate) const HELD_AT: &str = "held_at";
    pub(crate) const STATUS: &str = "status";
    pub(crate) const OVERALL_LEVEL: &str = "overall_level";
    pub(crate) const ACTION: &str = "action";
    pub(crate) const TERMS: &str = "terms";
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const MODERATOR_ID: &str = "id";
    pub(crate) const MODERATOR_LOGIN: &str = "login";
    pub(crate) const MODERATOR_DISPLAY_NAME: &str = "display_name";
    pub(crate) const MESSAGE_TEXT: &str = "message_text";
}

pub(crate) mod warning {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const MODERATOR_LOGIN: &str = "login";
    pub(crate) const REASON: &str = "reason";
    pub(crate) const CHAT_RULES_CITED: &str = "chat_rules_cited";
}

pub(crate) mod vip {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod unban_request {
    pub(crate) const REQUEST_ID: &str = "id";
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const REASON_TEXT: &str = "reason_text";
    pub(crate) const STATUS: &str = "status";
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const MODERATOR_ID: &str = "id";
    pub(crate) const MODERATOR_LOGIN: &str = "login";
    pub(crate) const MODERATOR_DISPLAY_NAME: &str = "display_name";
    pub(crate) const RESOLUTION_TEXT: &str = "resolution_text";
}

pub(crate) mod stream {
    pub(crate) const STREAM: &str = "stream";
    pub(crate) const STREAM_ID: &str = "id";
    pub(crate) const STREAM_TYPE: &str = "type";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const BROADCASTER: &str = "broadcaster";
    pub(crate) const BROADCASTER_ID: &str = "id";
    pub(crate) const BROADCASTER_LOGIN: &str = "login";
}

pub(crate) mod shoutout {
    pub(crate) const TO_BROADCASTER: &str = "to_broadcaster";
    pub(crate) const FROM_BROADCASTER: &str = "from_broadcaster";
    pub(crate) const BROADCASTER_ID: &str = "id";
    pub(crate) const BROADCASTER_LOGIN: &str = "login";
    pub(crate) const BROADCASTER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const VIEWER_COUNT: &str = "viewer_count";
    pub(crate) const STARTED_AT: &str = "started_at";
}

pub(crate) mod shield {
    pub(crate) const MODERATOR: &str = "moderator";
    pub(crate) const MODERATOR_ID: &str = "id";
    pub(crate) const MODERATOR_LOGIN: &str = "login";
    pub(crate) const MODERATOR_DISPLAY_NAME: &str = "display_name";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const ENDED_AT: &str = "ended_at";
}

pub(crate) mod moderator {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
}

pub(crate) mod whisper {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const USER_COLOR: &str = "color";
    pub(crate) const WHISPER: &str = "whisper";
    pub(crate) const WHISPER_TEXT: &str = "text";
    pub(crate) const WHISPER_THREAD_ID: &str = "whisper_thread_id";
}

pub(crate) mod user {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const USER_DESCRIPTION: &str = "description";
}

pub(crate) mod suspicious {
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const LOW_TRUST_STATUS: &str = "low_trust_status";
    pub(crate) const MESSAGE_TEXT: &str = "message_text";
}

pub(crate) mod automatic_reward {
    pub(crate) const REDEMPTION: &str = "redemption";
    pub(crate) const REDEMPTION_ID: &str = "id";
    pub(crate) const REDEEMED_AT: &str = "redeemed_at";
    pub(crate) const USER: &str = "user";
    pub(crate) const USER_ID: &str = "id";
    pub(crate) const USER_LOGIN: &str = "login";
    pub(crate) const USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const REWARD: &str = "reward";
    pub(crate) const REWARD_TYPE: &str = "type";
    pub(crate) const REWARD_COST: &str = "cost";
}

pub(crate) mod ad_break {
    pub(crate) const AD_BREAK: &str = "ad_break";
    pub(crate) const DURATION_SECONDS: &str = "duration_seconds";
    pub(crate) const IS_AUTOMATIC: &str = "is_automatic";
    pub(crate) const STARTED_AT: &str = "started_at";
    pub(crate) const REQUESTER: &str = "requester";
    pub(crate) const REQUESTER_LOGIN: &str = "login";
}

pub(crate) mod chat_mod {
    pub(crate) const BROADCASTER: &str = "broadcaster";
    pub(crate) const BROADCASTER_ID: &str = "id";
    pub(crate) const BROADCASTER_LOGIN: &str = "login";
    pub(crate) const MESSAGE_ID: &str = "message_id";
    pub(crate) const TARGET_USER: &str = "target_user";
    pub(crate) const TARGET_USER_ID: &str = "id";
    pub(crate) const TARGET_USER_LOGIN: &str = "login";
    pub(crate) const TARGET_USER_DISPLAY_NAME: &str = "display_name";
    pub(crate) const SETTINGS: &str = "settings";
    pub(crate) const EMOTE_MODE: &str = "emote_mode";
    pub(crate) const FOLLOWER_MODE: &str = "follower_mode";
    pub(crate) const FOLLOWER_MODE_DURATION_MINUTES: &str = "follower_mode_duration_minutes";
    pub(crate) const SLOW_MODE: &str = "slow_mode";
    pub(crate) const SLOW_MODE_WAIT_TIME_SECONDS: &str = "slow_mode_wait_time_seconds";
    pub(crate) const SUBSCRIBER_MODE: &str = "subscriber_mode";
    pub(crate) const UNIQUE_CHAT_MODE: &str = "unique_chat_mode";
}
