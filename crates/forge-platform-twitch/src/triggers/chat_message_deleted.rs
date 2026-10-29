use crate::event_kinds;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, TriggerConfig, Variant,
    VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::chat_mod as chat_mod_fields;

pub(crate) struct ChatMessageDeletedDescriptor;

impl TriggerKindDescriptor for ChatMessageDeletedDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.message_deleted"
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
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
                .event_specific(
                    DeclaredVariable {
                        name: "chat.message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Deleted message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, chat_mod_fields::MESSAGE_ID)),
                )
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
    fn event_filter_gates_on_message_delete_kind_prefix() {
        let filter = ChatMessageDeletedDescriptor.event_filter();
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.chat.message_delete")
        );
        assert_eq!(filter.source, Some(EventSource::Twitch));
    }

    #[test]
    fn build_arg_stack_maps_message_id_and_target_user_from_nested_payload() {
        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&message_delete_event());
        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String("msg-abc-123".to_owned()))
        );
        assert_eq!(
            stack.get("chat.target_user.login"),
            Some(&Variant::String("spammer_user".to_owned()))
        );
        assert_eq!(
            stack.get("chat.target_user.id"),
            Some(&Variant::String("9001".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_to_empty_strings_when_fields_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.chat.message_delete",
            serde_json::json!({}),
        );
        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("chat.target_user.login"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("chat.target_user.id"),
            Some(&Variant::String(String::new()))
        );
    }
}
