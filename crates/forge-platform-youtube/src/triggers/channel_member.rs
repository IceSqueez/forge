use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::{chat as chat_fields, entity, member as fields};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let author = event.payload.get(chat_fields::AUTHOR);
        let user_display_name = author
            .and_then(|a| a.get(entity::DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let channel_id = author
            .and_then(|a| a.get(entity::CHANNEL_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let member_level_name = event
            .payload
            .get(fields::MEMBER_LEVEL_NAME)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "user_display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set("channel_id".to_owned(), Variant::String(channel_id))
            .set(
                "member_level_name".to_owned(),
                Variant::String(member_level_name),
            )
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "user_display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "New member display name".to_owned(),
                    synthesis: Some(SynthesisHint::DisplayName),
                },
                DeclaredVariable {
                    name: "channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "New member channel ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "member_level_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Membership level name".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
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
    fn always_matches() {
        assert!(
            SupportNewMemberDescriptor.matches_trigger(&TriggerConfig::new(), &new_member_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_member_fields() {
        let stack = SupportNewMemberDescriptor.build_arg_stack(&new_member_event());
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("NewSponsor".to_owned()))
        );
        assert_eq!(
            stack.get("member_level_name"),
            Some(&Variant::String("Bronze".to_owned()))
        );
    }
}
