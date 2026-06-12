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
];
