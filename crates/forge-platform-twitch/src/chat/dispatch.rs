use super::session::ChatSession;

pub(super) type NotificationRoute = fn(&ChatSession, &serde_json::Value, &str);

pub(super) fn route_for(subscription_type: &str) -> Option<NotificationRoute> {
    NOTIFICATION_ROUTES
        .iter()
        .find(|(topic, _)| *topic == subscription_type)
        .map(|(_, route)| *route)
}

const NOTIFICATION_ROUTES: &[(&str, NotificationRoute)] = &[
    ("channel.chat.message", |session, event_data, _| {
        session.publish_chat_message(event_data);
    }),
    ("channel.subscribe", ChatSession::publish_subscribe_event),
    (
        "channel.subscription.message",
        ChatSession::publish_resubscribe_event,
    ),
    (
        "channel.subscription.gift",
        ChatSession::publish_gift_sub_event,
    ),
    ("channel.cheer", ChatSession::publish_cheer_event),
    ("channel.raid", ChatSession::publish_raid_event),
    ("channel.follow", ChatSession::publish_follow_event),
    ("stream.online", ChatSession::publish_stream_online_event),
    ("stream.offline", ChatSession::publish_stream_offline_event),
    (
        "channel.channel_points_custom_reward_redemption.add",
        ChatSession::publish_reward_redemption_event,
    ),
    (
        "channel.chat.message_delete",
        ChatSession::publish_message_delete_event,
    ),
    ("channel.chat.clear", ChatSession::publish_chat_clear_event),
    (
        "channel.hype_train.begin",
        ChatSession::publish_hype_train_begin_event,
    ),
    (
        "channel.hype_train.progress",
        ChatSession::publish_hype_train_progress_event,
    ),
    (
        "channel.hype_train.end",
        ChatSession::publish_hype_train_end_event,
    ),
    (
        "channel.charity_campaign.donate",
        ChatSession::publish_charity_donation_event,
    ),
    (
        "channel.charity_campaign.start",
        ChatSession::publish_charity_start_event,
    ),
    (
        "channel.charity_campaign.progress",
        ChatSession::publish_charity_progress_event,
    ),
    (
        "channel.charity_campaign.stop",
        ChatSession::publish_charity_stop_event,
    ),
    ("channel.ban", ChatSession::publish_ban_event),
    ("channel.unban", ChatSession::publish_unban_event),
    (
        "channel.moderator.add",
        ChatSession::publish_moderator_add_event,
    ),
    (
        "channel.moderator.remove",
        ChatSession::publish_moderator_remove_event,
    ),
    ("channel.vip.add", ChatSession::publish_vip_add_event),
    ("channel.vip.remove", ChatSession::publish_vip_remove_event),
    (
        "channel.unban_request.create",
        ChatSession::publish_unban_request_create_event,
    ),
    (
        "channel.unban_request.resolve",
        ChatSession::publish_unban_request_resolve_event,
    ),
    (
        "channel.shield_mode.begin",
        ChatSession::publish_shield_mode_begin_event,
    ),
    (
        "channel.shield_mode.end",
        ChatSession::publish_shield_mode_end_event,
    ),
    (
        "channel.shoutout.create",
        ChatSession::publish_shoutout_create_event,
    ),
    (
        "channel.shoutout.receive",
        ChatSession::publish_shoutout_receive_event,
    ),
    (
        "channel.suspicious_user.message",
        ChatSession::publish_suspicious_user_event,
    ),
    (
        "channel.warning.acknowledge",
        ChatSession::publish_warning_acknowledge_event,
    ),
    (
        "channel.warning.send",
        ChatSession::publish_warning_send_event,
    ),
    ("channel.poll.begin", ChatSession::publish_poll_begin_event),
    (
        "channel.poll.progress",
        ChatSession::publish_poll_progress_event,
    ),
    ("channel.poll.end", ChatSession::publish_poll_end_event),
    (
        "channel.prediction.begin",
        ChatSession::publish_prediction_begin_event,
    ),
    (
        "channel.prediction.progress",
        ChatSession::publish_prediction_progress_event,
    ),
    (
        "channel.prediction.lock",
        ChatSession::publish_prediction_lock_event,
    ),
    (
        "channel.prediction.end",
        ChatSession::publish_prediction_end_event,
    ),
    ("channel.goal.begin", ChatSession::publish_goal_begin_event),
    (
        "channel.goal.progress",
        ChatSession::publish_goal_progress_event,
    ),
    ("channel.goal.end", ChatSession::publish_goal_end_event),
    (
        "channel.channel_points_custom_reward.add",
        ChatSession::publish_reward_add_event,
    ),
    (
        "channel.channel_points_custom_reward.update",
        ChatSession::publish_reward_update_event,
    ),
    (
        "channel.channel_points_custom_reward.remove",
        ChatSession::publish_reward_remove_event,
    ),
    (
        "channel.channel_points_custom_reward_redemption.update",
        ChatSession::publish_redemption_update_event,
    ),
    (
        "channel.automod.message.hold",
        ChatSession::publish_automod_hold_event,
    ),
    (
        "channel.chat_settings.update",
        ChatSession::publish_chat_settings_update_event,
    ),
    (
        "channel.guest_star_session.begin",
        ChatSession::publish_guest_star_session_begin_event,
    ),
    (
        "channel.guest_star_session.end",
        ChatSession::publish_guest_star_session_end_event,
    ),
    (
        "channel.guest_star_settings.update",
        ChatSession::publish_guest_star_settings_event,
    ),
    (
        "channel.guest_star_guest.update",
        ChatSession::publish_guest_star_guest_update_event,
    ),
    (
        "channel.guest_star_slot.update",
        ChatSession::publish_guest_star_slot_update_event,
    ),
    (
        "channel.automod.settings.update",
        ChatSession::publish_automod_settings_update_event,
    ),
    (
        "channel.automod.terms.update",
        ChatSession::publish_automod_terms_update_event,
    ),
    (
        "channel.automod.message.update",
        ChatSession::publish_automod_message_update_event,
    ),
    (
        "channel.shared_chat.begin",
        ChatSession::publish_shared_chat_begin_event,
    ),
    (
        "channel.shared_chat.update",
        ChatSession::publish_shared_chat_update_event,
    ),
    (
        "channel.shared_chat.end",
        ChatSession::publish_shared_chat_end_event,
    ),
    ("channel.update", ChatSession::publish_channel_update_event),
    (
        "channel.ad_break.begin",
        ChatSession::publish_ad_break_begin_event,
    ),
    (
        "channel.channel_points_automatic_reward_redemption.add",
        ChatSession::publish_automatic_reward_event,
    ),
    ("user.whisper.message", ChatSession::publish_whisper_event),
];

#[cfg(test)]
mod tests {
    use super::route_for;

    #[test]
    fn route_for_resolves_every_supported_topic_and_rejects_unknown() {
        for topic in [
            "channel.chat.message",
            "channel.subscribe",
            "channel.subscription.message",
            "channel.subscription.gift",
            "channel.cheer",
            "channel.raid",
            "channel.follow",
            "stream.online",
            "stream.offline",
            "channel.channel_points_custom_reward_redemption.add",
            "channel.chat.message_delete",
            "channel.chat.clear",
            "channel.hype_train.begin",
            "channel.hype_train.progress",
            "channel.hype_train.end",
            "channel.charity_campaign.donate",
            "channel.charity_campaign.start",
            "channel.charity_campaign.progress",
            "channel.charity_campaign.stop",
            "channel.ban",
            "channel.unban",
            "channel.moderator.add",
            "channel.moderator.remove",
            "channel.unban_request.create",
            "channel.unban_request.resolve",
            "channel.vip.add",
            "channel.vip.remove",
            "channel.shield_mode.begin",
            "channel.shield_mode.end",
            "channel.shoutout.create",
            "channel.shoutout.receive",
            "channel.suspicious_user.message",
            "channel.warning.acknowledge",
            "channel.warning.send",
            "channel.poll.begin",
            "channel.poll.progress",
            "channel.poll.end",
            "channel.prediction.begin",
            "channel.prediction.progress",
            "channel.prediction.lock",
            "channel.prediction.end",
            "channel.goal.begin",
            "channel.goal.progress",
            "channel.goal.end",
            "channel.channel_points_custom_reward.add",
            "channel.channel_points_custom_reward.update",
            "channel.channel_points_custom_reward.remove",
            "channel.channel_points_custom_reward_redemption.update",
            "channel.automod.message.hold",
            "channel.chat_settings.update",
            "channel.guest_star_session.begin",
            "channel.guest_star_session.end",
            "channel.guest_star_settings.update",
            "channel.guest_star_slot.update",
            "channel.guest_star_guest.update",
            "channel.automod.settings.update",
            "channel.automod.terms.update",
            "channel.automod.message.update",
            "channel.shared_chat.begin",
            "channel.shared_chat.update",
            "channel.shared_chat.end",
            "channel.update",
            "channel.ad_break.begin",
            "channel.channel_points_automatic_reward_redemption.add",
            "user.whisper.message",
        ] {
            assert!(route_for(topic).is_some(), "missing route for {topic}");
        }
        for unknown in ["channel.guest", "stream", ""] {
            assert!(
                route_for(unknown).is_none(),
                "unexpected route for {unknown}"
            );
        }
    }
}
