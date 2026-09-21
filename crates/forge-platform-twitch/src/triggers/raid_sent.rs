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

pub(crate) struct RaidSentDescriptor;

impl TriggerKindDescriptor for RaidSentDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.raid_sent"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Raids
    }

    fn label(&self) -> &str {
        "Raid sent"
    }

    fn summary(&self) -> &str {
        "Fires when you raid another streamer's channel"
    }

    fn search_text(&self) -> &str {
        "twitch raid outgoing send host viewers streamer"
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
            .is_some_and(|d| d == "sent")
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), raid_target_identity)
                .count(CanonicalCount::ViewerCount, |event| {
                    payload_read::number(event, fields::VIEWER_COUNT)
                })
                .legacy(
                    DeclaredVariable {
                        name: "to_broadcaster_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Target channel login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::TO_BROADCASTER,
                            fields::BROADCASTER_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "to_broadcaster_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Target channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            fields::TO_BROADCASTER,
                            fields::BROADCASTER_ID,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn raid_target_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::TO_BROADCASTER),
        fields::BROADCASTER_ID,
        fields::BROADCASTER_LOGIN,
        fields::BROADCASTER_DISPLAY_NAME,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_targets_channel_raid_topic_from_twitch() {
        let filter = RaidSentDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.raid"));
    }

    #[test]
    fn build_arg_stack_exposes_to_broadcaster_and_viewer_count_as_int() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.raid",
            serde_json::json!({
                "direction": "sent",
                "viewer_count": 37,
                "to_broadcaster": {
                    "id": "999",
                    "login": "target_chan",
                    "display_name": "TargetChan"
                }
            }),
        );
        let stack = RaidSentDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("to_broadcaster_login"),
            Some(&Variant::String("target_chan".to_owned()))
        );
        assert_eq!(
            stack.get("to_broadcaster_id"),
            Some(&Variant::String("999".to_owned()))
        );
        assert_eq!(stack.get("viewer_count"), Some(&Variant::Int(37)));
    }
}
