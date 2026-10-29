use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{PlatformId, TriggerConfig};

use super::payload_read::{self, kick_actor};
use crate::vocabulary::fields::chat as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct MessageDeletedDescriptor;

impl TriggerKindDescriptor for MessageDeletedDescriptor {
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
            kind_prefix: Some(event_kinds::CHAT_MESSAGE_DELETED.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), deleted_by_identity)
                .chat_message_id(|event| payload_read::text(event, fields::MESSAGE_ID))
                .legacy(
                    variables::legacy::DELETED_BY_ID,
                    "Moderator user ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
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
mod tests {
    use super::*;

    use forge_types::Variant;
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
            ("chat.message_id", "msg-uuid-999"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
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
                "chat.message_id".to_owned(),
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
