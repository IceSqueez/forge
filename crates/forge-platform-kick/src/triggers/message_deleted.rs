use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::{chat as fields, entity};

pub(crate) struct MessageDeletedDescriptor;

impl TriggerKindDescriptor for MessageDeletedDescriptor {
    fn id(&self) -> &str {
        "kick.chat.message.deleted"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Message deleted"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator deletes a chat message in Kick"
    }

    fn search_text(&self) -> &str {
        "kick message deleted removed moderated chat"
    }

    fn icon_name(&self) -> &str {
        "trash"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.chat.message.deleted".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let message_id = event
            .payload
            .get(fields::MESSAGE_ID)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let deleted_by_id = event
            .payload
            .get(fields::DELETED_BY)
            .and_then(|d| d.get(entity::ID))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        ArgStack::new()
            .set("message_id".to_owned(), Variant::String(message_id))
            .set("deleted_by_id".to_owned(), Variant::String(deleted_by_id))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "message_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Deleted message ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "deleted_by_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Moderator user ID".to_owned(),
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

    fn delete_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message.deleted",
            serde_json::json!({
                "message_id": "msg-uuid-999",
                "deleted_by": { "id": 5, "username": null }
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_ids() {
        let stack = MessageDeletedDescriptor.build_arg_stack(&delete_event());
        assert_eq!(
            stack.get("message_id"),
            Some(&Variant::String("msg-uuid-999".to_owned()))
        );
        assert_eq!(
            stack.get("deleted_by_id"),
            Some(&Variant::String("5".to_owned()))
        );
    }
}
