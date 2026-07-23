use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::chat_mod as fields;

pub(crate) struct ChatMessageDeletedDescriptor;

impl TriggerKindDescriptor for ChatMessageDeletedDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.message_deleted"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Message deleted"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator deletes a message from YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube chat message deleted removed moderation moderator"
    }

    fn icon_name(&self) -> &str {
        "trash-2"
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
            kind_prefix: Some("youtube.chat.message_deleted".to_owned()),
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

        ArgStack::new().set("chat.message_id".to_owned(), Variant::String(message_id))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "chat.message_id".to_owned(),
                kind: VariantKind::String,
                label: "Deleted message ID".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deleted_event(payload: serde_json::Value) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message_deleted",
            payload,
        )
    }

    #[test]
    fn build_arg_stack_surfaces_deleted_message_id() {
        let event = deleted_event(json!({ "message_id": "msg-removed-1" }));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String("msg-removed-1".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_defaults_message_id_to_empty() {
        let event = deleted_event(json!({}));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String(String::new()))
        );
    }
}
