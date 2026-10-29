use super::dispatch::NotificationRoute;
use super::session::ChatSession;
use crate::vocabulary::topics;

pub(super) struct TopicSpec {
    pub(super) kind: &'static str,
    pub(super) version: &'static str,
    pub(super) condition_fn: fn(&str, &str) -> serde_json::Value,
    pub(super) route: NotificationRoute,
}

fn condition_broadcaster(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ "broadcaster_user_id": broadcaster_id })
}

fn condition_chat(broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({
        "broadcaster_user_id": broadcaster_id,
        "user_id": user_id,
    })
}

fn condition_moderator(broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({
        "broadcaster_user_id": broadcaster_id,
        "moderator_user_id": user_id,
    })
}

pub(super) const RAID_TARGET_FIELD: &str = "to_broadcaster_user_id";
pub(super) const RAID_SOURCE_FIELD: &str = "from_broadcaster_user_id";

fn condition_raid(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ RAID_TARGET_FIELD: broadcaster_id })
}

fn condition_raid_sent(broadcaster_id: &str, _user_id: &str) -> serde_json::Value {
    serde_json::json!({ RAID_SOURCE_FIELD: broadcaster_id })
}

fn condition_user(_broadcaster_id: &str, user_id: &str) -> serde_json::Value {
    serde_json::json!({ "user_id": user_id })
}

pub(super) const TOPICS: &[TopicSpec] = &[
    TopicSpec {
        kind: topics::CHANNEL_CHAT_MESSAGE,
        version: "1",
        condition_fn: condition_chat,
        route: |session, event_data, _| {
            session.publish_chat_message(event_data);
        },
    },
    TopicSpec {
        kind: topics::CHANNEL_SUBSCRIBE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_subscribe_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SUBSCRIPTION_GIFT,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_gift_sub_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SUBSCRIPTION_MESSAGE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_resubscribe_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHEER,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_cheer_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_FOLLOW,
        version: "2",
        condition_fn: condition_moderator,
        route: ChatSession::publish_follow_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_RAID,
        version: "1",
        condition_fn: condition_raid,
        route: ChatSession::publish_raid_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_RAID,
        version: "1",
        condition_fn: condition_raid_sent,
        route: ChatSession::publish_raid_event,
    },
    TopicSpec {
        kind: topics::STREAM_ONLINE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_stream_online_event,
    },
    TopicSpec {
        kind: topics::STREAM_OFFLINE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_stream_offline_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_ADD,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_reward_redemption_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHAT_MESSAGE_DELETE,
        version: "1",
        condition_fn: condition_chat,
        route: ChatSession::publish_message_delete_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHAT_CLEAR,
        version: "1",
        condition_fn: condition_chat,
        route: ChatSession::publish_chat_clear_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_HYPE_TRAIN_BEGIN,
        version: "2",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_hype_train_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_HYPE_TRAIN_PROGRESS,
        version: "2",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_hype_train_progress_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_HYPE_TRAIN_END,
        version: "2",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_hype_train_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHARITY_CAMPAIGN_DONATE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_charity_donation_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHARITY_CAMPAIGN_START,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_charity_start_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHARITY_CAMPAIGN_PROGRESS,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_charity_progress_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHARITY_CAMPAIGN_STOP,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_charity_stop_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_BAN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_ban_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_UNBAN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_unban_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_MODERATOR_ADD,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_moderator_add_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_MODERATOR_REMOVE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_moderator_remove_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_VIP_ADD,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_vip_add_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_VIP_REMOVE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_vip_remove_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_UNBAN_REQUEST_CREATE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_unban_request_create_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_UNBAN_REQUEST_RESOLVE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_unban_request_resolve_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHIELD_MODE_BEGIN,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_shield_mode_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHIELD_MODE_END,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_shield_mode_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHOUTOUT_CREATE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_shoutout_create_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHOUTOUT_RECEIVE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_shoutout_receive_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SUSPICIOUS_USER_MESSAGE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_suspicious_user_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_WARNING_ACKNOWLEDGE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_warning_acknowledge_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_WARNING_SEND,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_warning_send_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_POLL_BEGIN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_poll_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_POLL_PROGRESS,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_poll_progress_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_POLL_END,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_poll_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_PREDICTION_BEGIN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_prediction_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_PREDICTION_PROGRESS,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_prediction_progress_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_PREDICTION_LOCK,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_prediction_lock_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_PREDICTION_END,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_prediction_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GOAL_BEGIN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_goal_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GOAL_PROGRESS,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_goal_progress_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GOAL_END,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_goal_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_ADD,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_reward_add_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_UPDATE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_reward_update_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REMOVE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_reward_remove_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_UPDATE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_redemption_update_event,
    },
    TopicSpec {
        kind: topics::AUTOMOD_MESSAGE_HOLD,
        version: "2",
        condition_fn: condition_moderator,
        route: ChatSession::publish_automod_hold_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHAT_SETTINGS_UPDATE,
        version: "1",
        condition_fn: condition_chat,
        route: ChatSession::publish_chat_settings_update_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GUEST_STAR_SESSION_BEGIN,
        version: "beta",
        condition_fn: condition_moderator,
        route: ChatSession::publish_guest_star_session_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GUEST_STAR_SESSION_END,
        version: "beta",
        condition_fn: condition_moderator,
        route: ChatSession::publish_guest_star_session_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GUEST_STAR_SETTINGS_UPDATE,
        version: "beta",
        condition_fn: condition_moderator,
        route: ChatSession::publish_guest_star_settings_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_GUEST_STAR_GUEST_UPDATE,
        version: "beta",
        condition_fn: condition_moderator,
        route: ChatSession::publish_guest_star_guest_update_event,
    },
    TopicSpec {
        kind: topics::AUTOMOD_SETTINGS_UPDATE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_automod_settings_update_event,
    },
    TopicSpec {
        kind: topics::AUTOMOD_TERMS_UPDATE,
        version: "1",
        condition_fn: condition_moderator,
        route: ChatSession::publish_automod_terms_update_event,
    },
    TopicSpec {
        kind: topics::AUTOMOD_MESSAGE_UPDATE,
        version: "2",
        condition_fn: condition_moderator,
        route: ChatSession::publish_automod_message_update_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHARED_CHAT_BEGIN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_shared_chat_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHARED_CHAT_UPDATE,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_shared_chat_update_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_SHARED_CHAT_END,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_shared_chat_end_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_UPDATE,
        version: "2",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_channel_update_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_AD_BREAK_BEGIN,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_ad_break_begin_event,
    },
    TopicSpec {
        kind: topics::CHANNEL_CHANNEL_POINTS_AUTOMATIC_REWARD_REDEMPTION_ADD,
        version: "1",
        condition_fn: condition_broadcaster,
        route: ChatSession::publish_automatic_reward_event,
    },
    TopicSpec {
        kind: topics::USER_WHISPER_MESSAGE,
        version: "1",
        condition_fn: condition_user,
        route: ChatSession::publish_whisper_event,
    },
    TopicSpec {
        kind: topics::USER_UPDATE,
        version: "1",
        condition_fn: condition_user,
        route: ChatSession::publish_user_update_event,
    },
];
