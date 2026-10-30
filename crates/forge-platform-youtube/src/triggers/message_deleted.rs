use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{PlatformId, TriggerConfig};

use super::payload_read;
use crate::vocabulary::fields::chat_mod as fields;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) struct ChatMessageDeletedDescriptor;

impl TriggerKindDescriptor for ChatMessageDeletedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_MESSAGE_DELETED
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

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::CHAT_MESSAGE_DELETED.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .chat_message_id(|event| payload_read::text(event, fields::MESSAGE_ID)),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;
    use serde_json::json;

    fn deleted_event(payload: serde_json::Value) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message_deleted",
            payload,
        )
    }

    #[test]
    fn a_deletion_names_the_message_it_removed() {
        let event = deleted_event(json!({ "message_id": "msg-removed-1" }));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        for name in ["message_id", "chat.message_id"] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String("msg-removed-1".to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn a_tombstone_offers_no_user_variables_because_it_names_no_author() {
        let event = deleted_event(json!({ "message_id": "msg-removed-1" }));
        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);
        let schema = ChatMessageDeletedDescriptor.output_schema().unwrap();

        for name in ["user_id", "user_name", "user_platform"] {
            assert_eq!(stack.get(name), None, "stack '{name}'");
            assert!(
                schema.variables.iter().all(|v| v.name != name),
                "schema '{name}'"
            );
        }
    }
}
