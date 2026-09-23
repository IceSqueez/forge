use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId,
    SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::raid as fields;

pub(crate) struct ChannelRaidReceivedDescriptor;

impl TriggerKindDescriptor for ChannelRaidReceivedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.raid_received"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Raids
    }

    fn label(&self) -> &str {
        "Raid received"
    }

    fn summary(&self) -> &str {
        "Fires when another streamer raids your channel"
    }

    fn search_text(&self) -> &str {
        "twitch raid incoming host viewers streamer"
    }

    fn icon_name(&self) -> &str {
        "sword"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.channel.raid".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event
            .payload
            .get(fields::DIRECTION)
            .and_then(|v| v.as_str())
            .is_some_and(|d| d == "received")
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), raider_identity)
                .count(CanonicalCount::ViewerCount, raid_viewer_count)
                .legacy(
                    DeclaredVariable {
                        name: "raid_viewer_count".to_owned(),
                        kind: VariantKind::Int,
                        label: "Raid viewer count".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 500 }),
                    },
                    CanonicalVariable::Count(CanonicalCount::ViewerCount),
                    |event| Variant::Int(raid_viewer_count(event)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "raider_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Raider login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(raider_field(event, fields::BROADCASTER_LOGIN)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "raider_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Raider ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(raider_field(event, fields::BROADCASTER_ID)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "raider_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Raider display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(raider_field(event, fields::BROADCASTER_DISPLAY_NAME)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn raider_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::FROM_BROADCASTER),
        fields::BROADCASTER_ID,
        fields::BROADCASTER_LOGIN,
        fields::BROADCASTER_DISPLAY_NAME,
    )
}

fn raider_field(event: &Event, key: &str) -> String {
    payload_read::nested_text(event, fields::FROM_BROADCASTER, key)
}

fn raid_viewer_count(event: &Event) -> i64 {
    payload_read::number(event, fields::VIEWER_COUNT)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::triggers::raid_sent::RaidSentDescriptor;

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
            ChannelRaidReceivedDescriptor.matches_trigger(&cfg, &received),
            "incoming raid must fire raid_received"
        );
        assert!(
            !RaidSentDescriptor.matches_trigger(&cfg, &received),
            "incoming raid must NOT fire raid_sent"
        );

        let sent = raid_event("sent", 100);
        assert!(
            !ChannelRaidReceivedDescriptor.matches_trigger(&cfg, &sent),
            "outgoing raid must NOT fire raid_received"
        );
        assert!(
            RaidSentDescriptor.matches_trigger(&cfg, &sent),
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
        assert!(!ChannelRaidReceivedDescriptor.matches_trigger(&cfg, &event));
        assert!(!RaidSentDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn a_received_raid_publishes_the_raider_as_the_canonical_actor_and_counts_its_viewers() {
        let stack = ChannelRaidReceivedDescriptor.build_arg_stack(&raid_event("received", 250));
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

    #[test]
    fn every_legacy_raid_name_still_carries_the_value_its_canonical_twin_carries() {
        let stack = ChannelRaidReceivedDescriptor.build_arg_stack(&raid_event("received", 250));
        for (legacy, value) in [
            ("raider_id", "666"),
            ("raider_login", "big_streamer"),
            ("raider_display_name", "BigStreamer"),
        ] {
            assert_eq!(
                stack.get(legacy),
                Some(&Variant::String(value.to_owned())),
                "'{legacy}'"
            );
        }
        assert_eq!(stack.get("raid_viewer_count"), Some(&Variant::Int(250)));
    }
}
