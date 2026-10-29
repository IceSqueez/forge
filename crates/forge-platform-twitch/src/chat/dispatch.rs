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
            "automod.message.hold",
            "channel.chat_settings.update",
            "channel.guest_star_session.begin",
            "channel.guest_star_session.end",
            "channel.guest_star_settings.update",
            "channel.guest_star_guest.update",
            "automod.settings.update",
            "automod.terms.update",
            "automod.message.update",
            "channel.shared_chat.begin",
            "channel.shared_chat.update",
            "channel.shared_chat.end",
            "channel.update",
            "channel.ad_break.begin",
            "channel.channel_points_automatic_reward_redemption.add",
            "user.whisper.message",
            "user.update",
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
