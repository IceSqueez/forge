use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::{chat as chat_fields, chat_mod as fields};

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

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), author_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "chat.message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Deleted message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::MESSAGE_ID)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(chat_fields::AUTHOR))
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
    fn a_deletion_names_the_message_it_removed() {
        let event = deleted_event(json!({ "message_id": "msg-removed-1" }));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String("msg-removed-1".to_owned()))
        );
    }

    #[test]
    fn the_principal_block_stays_empty_because_a_tombstone_names_no_author() {
        let event = deleted_event(json!({ "message_id": "msg-removed-1" }));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        for name in ["user_id", "user_name"] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
    }
}
