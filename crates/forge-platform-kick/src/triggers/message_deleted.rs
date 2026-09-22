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

    use serde_json::json;

    fn a_deletion_by_a_named_moderator() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message.deleted",
            json!({
                "message_id": "msg-uuid-999",
                "deleted_by": { "id": 5, "username": "the_mod" }
            }),
        )
    }

    #[test]
    fn the_principal_is_the_deleter_because_the_wire_never_names_the_author() {
        let stack = MessageDeletedDescriptor.build_arg_stack(&a_deletion_by_a_named_moderator());
        for (name, value) in [
            ("user_id", "5"),
            ("user_login", "the_mod"),
            ("user_name", "the_mod"),
            ("message_id", "msg-uuid-999"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.get("deleted_by_id"), stack.get("user_id"));
    }

    #[test]
    fn a_deletion_publishes_nothing_beyond_the_message_id_and_the_deleter() {
        let published: Vec<String> = MessageDeletedDescriptor
            .build_arg_stack(&a_deletion_by_a_named_moderator())
            .snapshot()
            .into_keys()
            .collect();
        assert_eq!(
            published,
            vec![
                "deleted_by_id".to_owned(),
                "message_id".to_owned(),
                "user_id".to_owned(),
                "user_login".to_owned(),
                "user_name".to_owned(),
                "user_platform".to_owned(),
            ]
        );
    }
}
