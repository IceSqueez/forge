use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{PlatformId, TriggerConfig};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::chat_mod as chat_mod_fields;

pub(crate) struct ChatMessageDeletedDescriptor;

impl TriggerKindDescriptor for ChatMessageDeletedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_MESSAGE_DELETED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat Message Deleted"
    }

    fn summary(&self) -> &str {
        "Fires when a chat message is deleted by a moderator or bot"
    }

    fn search_text(&self) -> &str {
        "twitch chat delete message removed moderation"
    }

    fn icon_name(&self) -> &str {
        "trash"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::CHANNEL_CHAT_MESSAGE_DELETE.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), deleted_author_identity)
                .chat_message_id(|event| payload_read::text(event, chat_mod_fields::MESSAGE_ID))
                .legacy(
                    "chat.target_user.login",
                    "Message author login",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                )
                .legacy(
                    "chat.target_user.id",
                    "Message author ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn deleted_author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(chat_mod_fields::TARGET_USER),
        chat_mod_fields::TARGET_USER_ID,
        chat_mod_fields::TARGET_USER_LOGIN,
        chat_mod_fields::TARGET_USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;

    fn message_delete_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.chat.message_delete",
            serde_json::json!({
                "message_id": "msg-abc-123",
                "target_user": {
                    "id": "9001",
                    "login": "spammer_user",
                    "display_name": "SpammerUser"
                }
            }),
        )
    }

    #[test]
    fn build_arg_stack_maps_message_id_and_target_user_from_nested_payload() {
        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&message_delete_event());
        for (name, value) in [
            ("message_id", "msg-abc-123"),
            ("chat.message_id", "msg-abc-123"),
            ("chat.target_user.login", "spammer_user"),
            ("chat.target_user.id", "9001"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn build_arg_stack_defaults_to_empty_strings_when_fields_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.chat.message_delete",
            serde_json::json!({}),
        );
        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);
        for name in [
            "message_id",
            "chat.message_id",
            "chat.target_user.login",
            "chat.target_user.id",
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
    }
}
