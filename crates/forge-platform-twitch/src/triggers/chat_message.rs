use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChatMessageDescriptor;

impl TriggerKindDescriptor for ChatMessageDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.message"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in chat"
    }

    fn search_text(&self) -> &str {
        "twitch chat message trigger any incoming"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
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
            kind_prefix: Some("chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let message_text = event
            .payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = event
            .payload
            .get("user")
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = event
            .payload
            .get("user")
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let channel = event
            .payload
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let color = event
            .payload
            .get("color")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("message_text".to_owned(), Variant::String(message_text))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("channel".to_owned(), Variant::String(channel))
            .set("user_color".to_owned(), Variant::String(color))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chat_event(msg: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "channel": "mychannel",
                "user": { "login": "bob", "id": "456", "roles": [] },
                "message": msg,
                "badges": [],
                "color": "#FF0000"
            }),
        )
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(ChatMessageDescriptor.id(), "twitch.chat.message");
    }

    #[test]
    fn chat_message_descriptor_is_platform_specific_twitch() {
        assert_eq!(
            ChatMessageDescriptor.platform_contract(),
            KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
        );
    }

    #[test]
    fn always_matches() {
        let cfg = TriggerConfig::new();
        assert!(ChatMessageDescriptor.matches_trigger(&cfg, &chat_event("hello")));
        assert!(ChatMessageDescriptor.matches_trigger(&cfg, &chat_event("")));
    }

    #[test]
    fn condition_display_is_any() {
        assert_eq!(
            ChatMessageDescriptor.condition_display(&TriggerConfig::new()),
            "any"
        );
    }

    #[test]
    fn build_arg_stack_extracts_fields() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event("hi there"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hi there".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("bob".to_owned()))
        );
        assert_eq!(
            stack.get("channel"),
            Some(&Variant::String("mychannel".to_owned()))
        );
        assert_eq!(
            stack.get("user_color"),
            Some(&Variant::String("#FF0000".to_owned()))
        );
    }
}
