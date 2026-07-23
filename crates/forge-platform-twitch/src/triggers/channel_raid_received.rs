use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let viewer_count = event
            .payload
            .get(fields::VIEWER_COUNT)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let from_login = event
            .payload
            .get(fields::FROM_BROADCASTER)
            .and_then(|b| b.get(fields::BROADCASTER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_id = event
            .payload
            .get(fields::FROM_BROADCASTER)
            .and_then(|b| b.get(fields::BROADCASTER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_display_name = event
            .payload
            .get(fields::FROM_BROADCASTER)
            .and_then(|b| b.get(fields::BROADCASTER_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("raid_viewer_count".to_owned(), Variant::Int(viewer_count))
            .set("raider_login".to_owned(), Variant::String(from_login))
            .set("raider_id".to_owned(), Variant::String(from_id))
            .set(
                "raider_display_name".to_owned(),
                Variant::String(from_display_name),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "raid_viewer_count".to_owned(),
                        kind: VariantKind::Int,
                        label: "Raid viewer count".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 500 }),
                    },
                    DeclaredVariable {
                        name: "raider_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Raider login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "raider_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Raider ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "raider_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Raider display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                ],
            }
        })
    }
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
    fn build_arg_stack_extracts_raider_fields() {
        let stack = ChannelRaidReceivedDescriptor.build_arg_stack(&raid_event("received", 250));
        assert_eq!(stack.get("raid_viewer_count"), Some(&Variant::Int(250)));
        assert_eq!(
            stack.get("raider_login"),
            Some(&Variant::String("big_streamer".to_owned()))
        );
        assert_eq!(
            stack.get("raider_display_name"),
            Some(&Variant::String("BigStreamer".to_owned()))
        );
    }
}
