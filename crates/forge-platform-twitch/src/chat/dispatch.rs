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
        ] {
            assert!(route_for(topic).is_some(), "missing route for {topic}");
        }
        for unknown in ["channel.ban", "stream", ""] {
            assert!(
                route_for(unknown).is_none(),
                "unexpected route for {unknown}"
            );
        }
    }
}
