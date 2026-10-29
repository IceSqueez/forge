use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{PlatformId, TriggerConfig};

use super::payload_read::{self, youtube_actor};
use crate::vocabulary::fields::{chat as chat_fields, member as fields};
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct SupportNewMemberDescriptor;

impl TriggerKindDescriptor for SupportNewMemberDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_MEMBER
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Subscriptions
    }

    fn label(&self) -> &str {
        "New member"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer joins as a new YouTube channel member"
    }

    fn search_text(&self) -> &str {
        "youtube new member join sponsor subscription level"
    }

    fn icon_name(&self) -> &str {
        "user-plus"
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
            kind_prefix: Some(event_kinds::CHANNEL_MEMBER.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), member_identity)
                .sub_tier(|event| payload_read::text(event, fields::MEMBER_LEVEL_NAME))
                .legacy(
                    variables::sender::DISPLAY_NAME,
                    "New member display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy(
                    variables::sender::CHANNEL_ID,
                    "New member channel ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                )
                .legacy(
                    variables::member::LEVEL_NAME,
                    "Membership level name",
                    CanonicalVariable::SubTier,
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

    fn new_member_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.channel.member",
            serde_json::json!({
                "author": { "display_name": "NewSponsor", "channel_id": "UCsponsor" },
                "member_level_name": "Bronze"
            }),
        )
    }

    #[test]
    fn a_new_member_publishes_the_joiner_and_the_level_they_bought_as_the_sub_tier() {
        let stack = SupportNewMemberDescriptor.build_arg_stack(&new_member_event());
        for (name, value) in [
            ("user_id", "UCsponsor"),
            ("user_name", "NewSponsor"),
            ("sub_tier", "Bronze"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }
}
