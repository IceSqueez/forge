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

    fn a_batch_of_five() -> Event {
        gift_event(json!({
            "count": 5_i64,
            "level_name": "Diamond",
            "gifter": { "channel_id": "UCgifter", "display_name": "Generous" },
        }))
    }

    #[test]
    fn the_gifter_is_published_as_both_the_principal_and_the_gifter_role() {
        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&a_batch_of_five());
        for (name, value) in [
            ("user_id", "UCgifter"),
            ("user_name", "Generous"),
            ("gifter_id", "UCgifter"),
            ("gifter_name", "Generous"),
            ("sub_tier", "Diamond"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn the_gift_count_comes_from_the_batch_size_the_wire_reports() {
        for (wire_count, expected) in [
            (json!(5_i64), 5),
            (json!(1_i64), 1),
            (json!(null), 0),
            (json!("5"), 0),
        ] {
            let event = gift_event(json!({
                "count": wire_count.clone(),
                "level_name": "Diamond",
            }));
            assert_eq!(
                ChannelMemberGiftDescriptor
                    .build_arg_stack(&event)
                    .get("gift_count"),
                Some(&Variant::Int(expected)),
                "wire count {wire_count}"
            );
        }
    }

    #[test]
    fn the_legacy_gift_names_still_carry_what_their_canonical_twins_carry() {
        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&a_batch_of_five());
        assert_eq!(stack.get("gift.count"), stack.get("gift_count"));
        assert_eq!(stack.get("gift.level_name"), stack.get("sub_tier"));
        assert_eq!(stack.get("gifter.channel_id"), stack.get("gifter_id"));
        assert_eq!(stack.get("gifter.display_name"), stack.get("gifter_name"));
        assert_eq!(stack.get("gift.count"), Some(&Variant::Int(5)));
    }

    #[test]
    fn a_gift_batch_the_wire_leaves_blank_names_nobody_and_counts_nothing() {
        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&gift_event(json!({})));
        for name in [
            "user_id",
            "user_name",
            "gifter_id",
            "gifter_name",
            "sub_tier",
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("gift_count"), Some(&Variant::Int(0)));
    }
}
