use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, TriggerConfig, Variant,
    VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::payload_fields::chat as fields;

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

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), deleted_by_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Deleted message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::MESSAGE_ID)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "deleted_by_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator user ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(deleted_by_identity(event).id),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn deleted_by_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::DELETED_BY))
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
