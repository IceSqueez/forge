use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, PlatformId, TriggerConfig,
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
                    "user_display_name",
                    "Member display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy(
                    "channel_id",
                    "Member channel ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                )
                .legacy(
                    "member_month",
                    "Membership length in months",
                    CanonicalVariable::Count(CanonicalCount::SubCumulativeMonths),
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
mod tests {
    use super::*;
    use forge_types::Variant;

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
}
