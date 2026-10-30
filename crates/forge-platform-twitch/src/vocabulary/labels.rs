use forge_types::MessageCode;

use crate::vocabulary::PLATFORM_ID;

pub(crate) const AD_120_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_120_seconds");
pub(crate) const AD_150_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_150_seconds");
pub(crate) const AD_180_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_180_seconds");
pub(crate) const AD_30_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_30_seconds");
pub(crate) const AD_60_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_60_seconds");
pub(crate) const AD_90_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_90_seconds");
pub(crate) const AD_COOLDOWN_HINT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_ad_cooldown_hint");
pub(crate) const ADD_MOD: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_add_mod");
pub(crate) const ADD_VIP: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_add_vip");
pub(crate) const AFFILIATE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_affiliate");
pub(crate) const ANNOUNCEMENT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_announcement");
pub(crate) const ANNOUNCEMENT_TEXT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_announcement_text");
pub(crate) const BACKGROUND_COLOR: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_background_color");
pub(crate) const BAN_USER: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_ban_user");
pub(crate) const BLUE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_blue");
pub(crate) const CANCEL_POLL: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_cancel_poll");
pub(crate) const CANCEL_RAID: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_cancel_raid");
pub(crate) const CANCEL_REFUND: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_cancel_refund");
pub(crate) const CATEGORY: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_category");
pub(crate) const CATEGORY_EXAMPLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_category_example");
pub(crate) const CHANNEL: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_channel");
pub(crate) const CHANNEL_NAME: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_channel_name");
pub(crate) const CHANNEL_POINT_REWARDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_channel_point_rewards");
pub(crate) const CHANNEL_POINTS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_channel_points");
pub(crate) const CHANNEL_POINTS_VOTING: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_channel_points_voting");
pub(crate) const CHAT: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_chat");
pub(crate) const CHOICES_ONE_PER_LINE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_choices_one_per_line");
pub(crate) const CLEAR_CHAT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_clear_chat");
pub(crate) const COLOR: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_color");
pub(crate) const CREATE_STREAM_MARKER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_create_stream_marker");
pub(crate) const CREATED_OUTSIDE_FORGE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_created_outside_forge");
pub(crate) const DURATION: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_duration");
pub(crate) const DURATION_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_duration_seconds");
pub(crate) const EMOTE_ONLY_MODE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_emote_only_mode");
pub(crate) const ENABLED: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_enabled");
pub(crate) const END_POLL_NOW: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_end_poll_now");
pub(crate) const END_SHIELD_MODE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_end_shield_mode");
pub(crate) const EVENTSUB_SUBSCRIPTIONS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_eventsub_subscriptions");
pub(crate) const FOLLOWERS_ONLY_MODE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_followers_only_mode");
pub(crate) const FULFILL_REDEMPTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_fulfill_redemption");
pub(crate) const GLOBAL_COOLDOWN: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_global_cooldown");
pub(crate) const GREEN: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_green");
pub(crate) const LOCK_PREDICTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_lock_prediction");
pub(crate) const MANAGE_REWARDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_manage_rewards");
pub(crate) const MARKER_NOTE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_marker_note");
pub(crate) const MAX_PER_STREAM: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_max_per_stream");
pub(crate) const MAX_PER_VIEWER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_max_per_viewer");
pub(crate) const MESSAGE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_message");
pub(crate) const MIN_ACCOUNT_AGE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_min_account_age");
pub(crate) const MODERATION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_moderation");
pub(crate) const OAUTH_SCOPES: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_oauth_scopes");
pub(crate) const OFF: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_off");
pub(crate) const ON: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_on");
pub(crate) const ONE_CHOICE_PER_LINE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_one_choice_per_line");
pub(crate) const ONE_OUTCOME_PER_LINE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_one_outcome_per_line");
pub(crate) const OPTIONAL: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_optional");
pub(crate) const ORANGE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_orange");
pub(crate) const OUTCOMES_ONE_PER_LINE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_outcomes_one_per_line");
pub(crate) const PARTNER: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_partner");
pub(crate) const PAUSED: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_paused");
pub(crate) const QUEUED_REWARD_HINT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_queued_reward_hint");
pub(crate) const POLL: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_poll");
pub(crate) const POLL_QUESTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_poll_question");
pub(crate) const POLLS: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_polls");
pub(crate) const PREDICTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_prediction");
pub(crate) const PREDICTION_QUESTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_prediction_question");
pub(crate) const PREDICTIONS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_predictions");
pub(crate) const PRIMARY: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_primary");
pub(crate) const PROMPT: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_prompt");
pub(crate) const PURPLE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_purple");
pub(crate) const QUESTION: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_question");
pub(crate) const RAID_TARGET: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_raid_target");
pub(crate) const RAIDS_ADS: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_raids_ads");
pub(crate) const REASON: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_reason");
pub(crate) const REDEMPTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_redemption");
pub(crate) const REJECT_REFUND: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_reject_refund");
pub(crate) const REMOVE_MOD: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_remove_mod");
pub(crate) const REMOVE_VIP: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_remove_vip");
pub(crate) const REQUIRE_VIEWER_INPUT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_require_viewer_input");
pub(crate) const RESOLVE_PAY_OUT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_resolve_pay_out");
pub(crate) const RETRY_PENDING: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_retry_pending");
pub(crate) const REWARD: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_reward");
pub(crate) const REWARD_COST: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_reward_cost");
pub(crate) const REWARD_TITLE_PLACEHOLDER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_reward_title_placeholder");
pub(crate) const RUN_COMMERCIAL: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_run_commercial");
pub(crate) const SEND_MESSAGE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_send_message");
pub(crate) const SEND_SHOUTOUT: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_send_shoutout");
pub(crate) const SET_CATEGORY_GAME: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_set_category_game");
pub(crate) const SHIELD_SCOPE_MISSING: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_shield_scope_missing");
pub(crate) const SLOW_MODE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_slow_mode");
pub(crate) const SNOOZE_NEXT_AD: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_snooze_next_ad");
pub(crate) const STANDARD: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_standard");
pub(crate) const START_POLL: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_start_poll");
pub(crate) const START_PREDICTION: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_start_prediction");
pub(crate) const START_RAID: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_start_raid");
pub(crate) const START_SHIELD_MODE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_start_shield_mode");
pub(crate) const STREAM_INFO: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_stream_info");
pub(crate) const STREAM_TITLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_stream_title");
pub(crate) const TAGS_COMMA_SEP: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_tags_comma_sep");
pub(crate) const TAGS_EXAMPLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_tags_example");
pub(crate) const TIER_LOCKED: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_tier_locked");
pub(crate) const TIMEOUT_USER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_timeout_user");
pub(crate) const TITLE: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_title");
pub(crate) const UNBAN_USER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_unban_user");
pub(crate) const UNCHANGED: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_unchanged");
pub(crate) const UPDATE_TAGS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_update_tags");
pub(crate) const UPDATE_TITLE: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_update_title");
pub(crate) const USERNAME: MessageCode = MessageCode::from_static(PLATFORM_ID, "label_username");
pub(crate) const USERNAME_PLACEHOLDER: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_username_placeholder");
pub(crate) const WAIT_TIME_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_wait_time_seconds");
pub(crate) const WINDOW_SECONDS: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_window_seconds");
pub(crate) const SUBSCRIPTION_DROPPED: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_subscription_dropped");
pub(crate) const WINNING_OUTCOME: MessageCode =
    MessageCode::from_static(PLATFORM_ID, "label_winning_outcome");
