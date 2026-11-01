use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, TriggerCategory,
    TriggerVariables,
};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::chat_mod as chat_mod_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), deleted_author_identity)
            .chat_message_id(|event| payload_read::text(event, chat_mod_fields::MESSAGE_ID))
            .legacy(
                "chat.target_user.login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "chat.target_user.id",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHAT_MESSAGE_DELETED,
    category: TriggerCategory::Chat,
    label: "Chat Message Deleted",
    summary: "Fires when a chat message is deleted by a moderator or bot",
    search_text: "twitch chat delete message removed moderation",
    icon: "trash",
    kind: event_kinds::CHANNEL_CHAT_MESSAGE_DELETE,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
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
        let stack = SPEC.build_arg_stack(&message_delete_event());
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
        let stack = SPEC.build_arg_stack(&event);
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
