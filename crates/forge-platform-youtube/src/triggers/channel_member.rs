use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::{chat as chat_fields, member as fields};

pub(crate) struct SupportNewMemberDescriptor;

impl TriggerKindDescriptor for SupportNewMemberDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.member"
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
            kind_prefix: Some("youtube.channel.member".to_owned()),
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
                    DeclaredVariable {
                        name: "user_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "New member display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(member_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "New member channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(member_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "member_level_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Membership level name".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::SubTier,
                    |event| Variant::String(payload_read::text(event, fields::MEMBER_LEVEL_NAME)),
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

    #[test]
    fn the_legacy_member_names_still_carry_what_their_canonical_twins_carry() {
        let stack = SupportNewMemberDescriptor.build_arg_stack(&new_member_event());
        assert_eq!(stack.get("user_display_name"), stack.get("user_name"));
        assert_eq!(stack.get("channel_id"), stack.get("user_id"));
        assert_eq!(stack.get("member_level_name"), stack.get("sub_tier"));
        assert_eq!(
            stack.get("member_level_name"),
            Some(&Variant::String("Bronze".to_owned()))
        );
    }
}
