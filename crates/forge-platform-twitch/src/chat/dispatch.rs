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
            "channel.shield_mode.begin",
            "channel.shield_mode.end",
            "channel.shoutout.create",
            "channel.shoutout.receive",
            "channel.suspicious_user.message",
            "channel.warning.acknowledge",
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
