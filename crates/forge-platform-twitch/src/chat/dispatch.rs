use super::session::ChatSession;
use super::topic_catalog::TOPICS;

pub(super) type NotificationRoute = fn(&ChatSession, &serde_json::Value, &str);

pub(super) fn route_for(subscription_type: &str) -> Option<NotificationRoute> {
    TOPICS
        .iter()
        .find(|topic| topic.kind == subscription_type)
        .map(|topic| topic.route)
}

#[cfg(test)]
mod tests {
    use super::route_for;

    #[test]
    fn route_for_resolves_every_pinned_subscription_type_and_rejects_unknown() {
        let pinned: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/eventsub_subscriptions.json"
        ))
        .unwrap();
        for row in &pinned {
            let topic = row["type"].as_str().unwrap();
            assert!(route_for(topic).is_some(), "missing route for {topic}");
        }
        for unknown in ["channel.guest", "stream", ""] {
            assert!(
                route_for(unknown).is_none(),
                "unexpected route for {unknown}"
            );
        }
    }

    #[test]
    fn automod_wire_types_route_without_the_channel_prefix() {
        for prefixed in [
            "channel.automod.message.hold",
            "channel.automod.message.update",
            "channel.automod.settings.update",
            "channel.automod.terms.update",
        ] {
            assert!(
                route_for(prefixed).is_none(),
                "{prefixed} must not route; Twitch's automod subscription types carry no channel. prefix"
            );
        }
        for official in [
            "automod.message.hold",
            "automod.message.update",
            "automod.settings.update",
            "automod.terms.update",
        ] {
            assert!(
                route_for(official).is_some(),
                "missing route for {official}"
            );
        }
    }

    #[test]
    fn removed_guest_star_slot_wire_type_no_longer_routes() {
        assert!(route_for("channel.guest_star_slot.update").is_none());
    }
}
