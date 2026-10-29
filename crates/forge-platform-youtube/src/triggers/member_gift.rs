use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalCount, CanonicalVariable,
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
    TriggerVariables,
};
use forge_types::{PlatformId, TriggerConfig};

use super::payload_read::{self, youtube_actor};
use crate::vocabulary::fields::gift as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ChannelMemberGiftDescriptor;

impl TriggerKindDescriptor for ChannelMemberGiftDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_MEMBER_GIFT
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
            kind_prefix: Some(event_kinds::CHANNEL_MEMBER_GIFT.to_owned()),
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
                    variables::gift::COUNT,
                    "Memberships gifted count",
                    CanonicalVariable::Count(CanonicalCount::GiftCount),
                )
                .legacy(
                    variables::gift::LEVEL_NAME,
                    "Membership level name",
                    CanonicalVariable::SubTier,
                )
                .legacy(
                    variables::gift::GIFTER_CHANNEL_ID,
                    "Gifter channel ID",
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Id),
                )
                .legacy(
                    variables::gift::GIFTER_DISPLAY_NAME,
                    "Gifter display name",
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Name),
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
mod tests {
    use super::*;
    use forge_types::Variant;
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
    fn the_gift_count_comes_from_the_batch_size_floored_at_one() {
        for (wire_count, expected) in [
            (json!(5_i64), 5),
            (json!(1_i64), 1),
            (json!(0_i64), 1),
            (json!(-3_i64), 1),
            (json!(null), 1),
            (json!("5"), 1),
        ] {
            let event = gift_event(json!({
                "count": wire_count.clone(),
                "level_name": "Diamond",
            }));
            let stack = ChannelMemberGiftDescriptor.build_arg_stack(&event);
            assert_eq!(
                stack.get("gift_count"),
                Some(&Variant::Int(expected)),
                "wire count {wire_count}"
            );
        }
    }

    #[test]
    fn a_gift_batch_the_wire_leaves_blank_names_nobody_and_counts_one_gift() {
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
        for name in ["gift_count", "gift.count"] {
            assert_eq!(stack.get(name), Some(&Variant::Int(1)), "'{name}'");
        }
    }
}
