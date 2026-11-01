use super::spec::TwitchTriggerSpec;
use crate::vocabulary::RaidDirection;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalCount, CanonicalVariable,
    TriggerCategory, TriggerVariables,
};
use forge_types::TriggerConfig;

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::raid as fields;

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    event
        .payload
        .get(fields::DIRECTION)
        .and_then(|v| v.as_str())
        .is_some_and(|d| d == RaidDirection::Received.as_str())
}

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), raider_identity)
            .count(CanonicalCount::ViewerCount, raid_viewer_count)
            .legacy(
                "raid_viewer_count",
                "Raid viewer count",
                CanonicalVariable::Count(CanonicalCount::ViewerCount),
            )
            .legacy(
                "raider_login",
                "Raider login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "raider_id",
                "Raider ID",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
            )
            .legacy(
                "raider_display_name",
                "Raider display name",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_RAID_RECEIVED,
    category: TriggerCategory::Raids,
    label: "Raid received",
    summary: "Fires when another streamer raids your channel",
    search_text: "twitch raid incoming host viewers streamer",
    icon: "sword",
    kind: event_kinds::CHANNEL_RAID,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    ..TwitchTriggerSpec::BASE
};

fn raider_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::FROM_BROADCASTER),
        fields::BROADCASTER_ID,
        fields::BROADCASTER_LOGIN,
        fields::BROADCASTER_DISPLAY_NAME,
    )
}

fn raid_viewer_count(event: &Event) -> i64 {
    payload_read::number(event, fields::VIEWER_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triggers::raid_sent::SPEC as RAID_SENT;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn raid_event(direction: &str, viewers: i64) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.raid",
            serde_json::json!({
                "direction": direction,
                "viewer_count": viewers,
                "from_broadcaster": {
                    "id": "666",
                    "login": "big_streamer",
                    "display_name": "BigStreamer"
                },
                "to_broadcaster": {
                    "id": "1",
                    "login": "me",
                    "display_name": "Me"
                }
            }),
        )
    }

    #[test]
    fn direction_routes_received_and_sent_to_opposite_descriptors() {
        let cfg = TriggerConfig::new();

        let received = raid_event("received", 100);
        assert!(
            SPEC.matches_trigger(&cfg, &received),
            "incoming raid must fire raid_received"
        );
        assert!(
            !RAID_SENT.matches_trigger(&cfg, &received),
            "incoming raid must NOT fire raid_sent"
        );

        let sent = raid_event("sent", 100);
        assert!(
            !SPEC.matches_trigger(&cfg, &sent),
            "outgoing raid must NOT fire raid_received"
        );
        assert!(
            RAID_SENT.matches_trigger(&cfg, &sent),
            "outgoing raid must fire raid_sent"
        );
    }

    #[test]
    fn missing_direction_fires_neither_descriptor() {
        let cfg = TriggerConfig::new();
        let event = Event::new(
            EventSource::Twitch,
            "channel.raid",
            serde_json::json!({ "viewer_count": 5 }),
        );
        assert!(!SPEC.matches_trigger(&cfg, &event));
        assert!(!RAID_SENT.matches_trigger(&cfg, &event));
    }

    #[test]
    fn a_received_raid_publishes_the_raider_as_the_canonical_actor_and_counts_its_viewers() {
        let stack = SPEC.build_arg_stack(&raid_event("received", 250));
        for (name, value) in [
            ("user_id", "666"),
            ("user_name", "BigStreamer"),
            ("user_login", "big_streamer"),
            ("user_platform", "twitch"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("viewer_count"), Some(&Variant::Int(250)));
    }
}
