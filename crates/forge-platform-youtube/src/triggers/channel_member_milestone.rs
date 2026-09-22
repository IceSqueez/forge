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
use crate::payload_fields::{chat as chat_fields, member as fields};

pub(crate) struct SupportMemberMilestoneDescriptor;

impl TriggerKindDescriptor for SupportMemberMilestoneDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.member_milestone"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "Member milestone"
    }

    fn summary(&self) -> &str {
        "Fires when a YouTube channel member reaches a membership milestone"
    }

    fn search_text(&self) -> &str {
        "youtube member milestone anniversary months subscription streak"
    }

    fn icon_name(&self) -> &str {
        "award"
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
            kind_prefix: Some("youtube.channel.member_milestone".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), member_identity)
                .message_text(|event| payload_read::text(event, chat_fields::MESSAGE_TEXT))
                .count(CanonicalCount::SubCumulativeMonths, |event| {
                    payload_read::number(event, fields::MEMBER_MONTH)
                })
                .legacy(
                    DeclaredVariable {
                        name: "user_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Member display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(member_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Member channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(member_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "member_month".to_owned(),
                        kind: VariantKind::Int,
                        label: "Membership length in months".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 120 }),
                    },
                    CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths),
                    |event| Variant::Int(payload_read::number(event, fields::MEMBER_MONTH)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn member_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(chat_fields::AUTHOR))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn milestone_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.channel.member_milestone",
            serde_json::json!({
                "author": { "display_name": "LongTimeFan", "channel_id": "UCfan" },
                "member_month": 12,
                "message_text": "One year!"
            }),
        )
    }

    #[test]
    fn a_milestone_publishes_the_member_their_note_and_the_months_as_a_cumulative_count() {
        let stack = SupportMemberMilestoneDescriptor.build_arg_stack(&milestone_event());
        for (name, value) in [
            ("user_id", "UCfan"),
            ("user_name", "LongTimeFan"),
            ("message_text", "One year!"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("sub_cumulative_months"), Some(&Variant::Int(12)));
    }

    #[test]
    fn the_legacy_milestone_names_still_carry_what_their_canonical_twins_carry() {
        let stack = SupportMemberMilestoneDescriptor.build_arg_stack(&milestone_event());
        assert_eq!(stack.get("user_display_name"), stack.get("user_name"));
        assert_eq!(stack.get("channel_id"), stack.get("user_id"));
        assert_eq!(
            stack.get("member_month"),
            stack.get("sub_cumulative_months")
        );
        assert_eq!(stack.get("member_month"), Some(&Variant::Int(12)));
    }
}
