use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

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
            kind_prefix: Some("channel.chat.message_delete".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let message_id = event
            .payload
            .get("message_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let target_user_login = event
            .payload
            .get("target_user")
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let target_user_id = event
            .payload
            .get("target_user")
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("chat.message_id".to_owned(), Variant::String(message_id))
            .set(
                "chat.target_user.login".to_owned(),
                Variant::String(target_user_login),
            )
            .set(
                "chat.target_user.id".to_owned(),
                Variant::String(target_user_id),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
            Some("channel.chat.message_delete")
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
