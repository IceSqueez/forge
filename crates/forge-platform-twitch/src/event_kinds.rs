pub const CHANNEL_CHAT_MESSAGE: &str = "twitch.channel.chat.message";
pub const CHANNEL_SUBSCRIBE: &str = "twitch.channel.subscribe";
pub const CHANNEL_SUBSCRIPTION_MESSAGE: &str = "twitch.channel.subscription.message";
pub const CHANNEL_SUBSCRIPTION_GIFT: &str = "twitch.channel.subscription.gift";
pub const CHANNEL_CHEER: &str = "twitch.channel.cheer";
pub const CHANNEL_RAID: &str = "twitch.channel.raid";
pub const CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_ADD: &str =
    "twitch.channel.channel_points_custom_reward_redemption.add";
pub const CHANNEL_CHAT_MESSAGE_DELETE: &str = "twitch.channel.chat.message_delete";
pub const CHANNEL_CHAT_CLEAR: &str = "twitch.channel.chat.clear";
pub const CHANNEL_FOLLOW: &str = "twitch.channel.follow";
pub const STREAM_ONLINE: &str = "twitch.stream.online";
pub const STREAM_OFFLINE: &str = "twitch.stream.offline";
pub const CHANNEL_HYPE_TRAIN_BEGIN: &str = "twitch.channel.hype_train.begin";
pub const CHANNEL_HYPE_TRAIN_PROGRESS: &str = "twitch.channel.hype_train.progress";
pub const CHANNEL_HYPE_TRAIN_END: &str = "twitch.channel.hype_train.end";
pub const CHANNEL_CHARITY_CAMPAIGN_DONATE: &str = "twitch.channel.charity_campaign.donate";
pub const CHANNEL_CHARITY_CAMPAIGN_START: &str = "twitch.channel.charity_campaign.start";
pub const CHANNEL_CHARITY_CAMPAIGN_PROGRESS: &str = "twitch.channel.charity_campaign.progress";
pub const CHANNEL_CHARITY_CAMPAIGN_STOP: &str = "twitch.channel.charity_campaign.stop";
pub const CHANNEL_BAN: &str = "twitch.channel.ban";
pub const CHANNEL_UNBAN: &str = "twitch.channel.unban";
pub const CHANNEL_MODERATOR_ADD: &str = "twitch.channel.moderator.add";
pub const CHANNEL_MODERATOR_REMOVE: &str = "twitch.channel.moderator.remove";
pub const CHANNEL_VIP_ADD: &str = "twitch.channel.vip.add";
pub const CHANNEL_VIP_REMOVE: &str = "twitch.channel.vip.remove";
pub const CHANNEL_UNBAN_REQUEST_CREATE: &str = "twitch.channel.unban_request.create";
pub const CHANNEL_UNBAN_REQUEST_RESOLVE: &str = "twitch.channel.unban_request.resolve";
pub const CHANNEL_SHIELD_MODE_BEGIN: &str = "twitch.channel.shield_mode.begin";
pub const CHANNEL_SHIELD_MODE_END: &str = "twitch.channel.shield_mode.end";
pub const CHANNEL_SHOUTOUT_CREATE: &str = "twitch.channel.shoutout.create";
pub const CHANNEL_SHOUTOUT_RECEIVE: &str = "twitch.channel.shoutout.receive";
pub const CHANNEL_SUSPICIOUS_USER_MESSAGE: &str = "twitch.channel.suspicious_user.message";
pub const CHANNEL_WARNING_ACKNOWLEDGE: &str = "twitch.channel.warning.acknowledge";
pub const CHANNEL_WARNING_SEND: &str = "twitch.channel.warning.send";
pub const CHANNEL_POLL_BEGIN: &str = "twitch.channel.poll.begin";
pub const CHANNEL_POLL_PROGRESS: &str = "twitch.channel.poll.progress";
pub const CHANNEL_POLL_END: &str = "twitch.channel.poll.end";
pub const CHANNEL_PREDICTION_BEGIN: &str = "twitch.channel.prediction.begin";
pub const CHANNEL_PREDICTION_PROGRESS: &str = "twitch.channel.prediction.progress";
pub const CHANNEL_PREDICTION_LOCK: &str = "twitch.channel.prediction.lock";
pub const CHANNEL_PREDICTION_END: &str = "twitch.channel.prediction.end";
pub const CHANNEL_GOAL_BEGIN: &str = "twitch.channel.goal.begin";
pub const CHANNEL_GOAL_END: &str = "twitch.channel.goal.end";
pub const CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_ADD: &str =
    "twitch.channel.channel_points_custom_reward.add";
pub const CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_UPDATE: &str =
    "twitch.channel.channel_points_custom_reward.update";
pub const CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REMOVE: &str =
    "twitch.channel.channel_points_custom_reward.remove";
pub const CHANNEL_CHANNEL_POINTS_CUSTOM_REWARD_REDEMPTION_UPDATE: &str =
    "twitch.channel.channel_points_custom_reward_redemption.update";
pub const AUTOMOD_MESSAGE_HOLD: &str = "twitch.automod.message.hold";
pub const CHANNEL_CHAT_SETTINGS_UPDATE: &str = "twitch.channel.chat_settings.update";
pub const CHANNEL_GUEST_STAR_SESSION_BEGIN: &str = "twitch.channel.guest_star_session.begin";
pub const CHANNEL_GUEST_STAR_SESSION_END: &str = "twitch.channel.guest_star_session.end";
pub const CHANNEL_GUEST_STAR_SETTINGS_UPDATE: &str = "twitch.channel.guest_star_settings.update";
pub const CHANNEL_GUEST_STAR_GUEST_UPDATE: &str = "twitch.channel.guest_star_guest.update";
pub const AUTOMOD_SETTINGS_UPDATE: &str = "twitch.automod.settings.update";
pub const AUTOMOD_TERMS_UPDATE: &str = "twitch.automod.terms.update";
pub const AUTOMOD_MESSAGE_UPDATE: &str = "twitch.automod.message.update";
pub const CHANNEL_SHARED_CHAT_BEGIN: &str = "twitch.channel.shared_chat.begin";
pub const CHANNEL_SHARED_CHAT_UPDATE: &str = "twitch.channel.shared_chat.update";
pub const CHANNEL_SHARED_CHAT_END: &str = "twitch.channel.shared_chat.end";
pub const CHANNEL_UPDATE: &str = "twitch.channel.update";
pub const CHANNEL_AD_BREAK_BEGIN: &str = "twitch.channel.ad_break.begin";
pub const CHANNEL_CHANNEL_POINTS_AUTOMATIC_REWARD_REDEMPTION_ADD: &str =
    "twitch.channel.channel_points_automatic_reward_redemption.add";
pub const USER_WHISPER_MESSAGE: &str = "twitch.user.whisper.message";
pub const USER_UPDATE: &str = "twitch.user.update";
pub const EVENTSUB_SUBSCRIPTION_CREATED: &str = "twitch.eventsub.subscription.created";
pub const CHANNEL_GOAL_PROGRESS: &str = "twitch.channel.goal.progress";

pub const DESCRIPTOR_REBUILDING_KINDS: [&str; 8] = [
    CHANNEL_POLL_BEGIN,
    CHANNEL_POLL_PROGRESS,
    CHANNEL_POLL_END,
    CHANNEL_PREDICTION_BEGIN,
    CHANNEL_PREDICTION_PROGRESS,
    CHANNEL_PREDICTION_LOCK,
    CHANNEL_PREDICTION_END,
    CHANNEL_RAID,
];
