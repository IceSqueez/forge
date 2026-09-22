use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, DeclaredVariable, PlatformId,
    SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::gift as fields;

pub(crate) struct ChannelMemberGiftDescriptor;

impl TriggerKindDescriptor for ChannelMemberGiftDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.member_gift"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "Memberships gifted"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer gifts a batch of YouTube channel memberships"
    }

    fn search_text(&self) -> &str {
        "youtube member gift gifted memberships sponsor subscription level batch"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.channel.member_gift".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), gifter_identity)
                .actor(youtube_actor(ActorRole::Gifter), gifter_identity)
                .count(CanonicalCount::GiftCount, |event| {
                    payload_read::number(event, fields::COUNT)
                })
                .sub_tier(|event| payload_read::text(event, fields::LEVEL_NAME))
                .legacy(
                    DeclaredVariable {
                        name: "gift.count".to_owned(),
                        kind: VariantKind::Int,
                        label: "Memberships gifted count".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 100 }),
                    },
                    CanonicalVariable::Count(CanonicalCount::GiftCount),
                    |event| Variant::Int(payload_read::number(event, fields::COUNT)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "gift.level_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Membership level name".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::SubTier,
                    |event| Variant::String(payload_read::text(event, fields::LEVEL_NAME)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "gifter.channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Id),
                    |event| Variant::String(gifter_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "gifter.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Name),
                    |event| Variant::String(gifter_identity(event).display_name),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Gifter])
    }
}

fn gifter_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::GIFTER))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gift_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::YouTube, "youtube.channel.member_gift", payload)
    }

    #[test]
    fn build_arg_stack_surfaces_count_level_and_gifter() {
        let event = gift_event(json!({
            "count": 5_i64,
            "level_name": "Diamond",
            "gifter": { "channel_id": "UCgifter", "display_name": "Generous" },
        }));

        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("gift.count"), Some(&Variant::Int(5)));
        assert_eq!(
            stack.get("gift.level_name"),
            Some(&Variant::String("Diamond".to_owned()))
        );
        assert_eq!(
            stack.get("gifter.channel_id"),
            Some(&Variant::String("UCgifter".to_owned()))
        );
        assert_eq!(
            stack.get("gifter.display_name"),
            Some(&Variant::String("Generous".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_defaults_count_to_zero_and_strings_empty() {
        let event = gift_event(json!({}));

        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("gift.count"), Some(&Variant::Int(0)));
        assert_eq!(
            stack.get("gift.level_name"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("gifter.channel_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("gifter.display_name"),
            Some(&Variant::String(String::new()))
        );
    }
}
