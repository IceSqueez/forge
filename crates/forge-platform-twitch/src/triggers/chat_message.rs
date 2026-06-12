use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig};

use super::chat_arg_stack::base_chat_args;

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
        base_chat_args(event)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::Variant;

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
    fn always_matches() {
        let cfg = TriggerConfig::new();
        assert!(ChatMessageDescriptor.matches_trigger(&cfg, &chat_event("hello")));
        assert!(ChatMessageDescriptor.matches_trigger(&cfg, &chat_event("")));
    }

    #[test]
    fn build_arg_stack_includes_base_chat_args() {
        // Full base-arg extraction is covered in chat_arg_stack tests;
        // this only guards the delegation.
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event("hi there"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hi there".to_owned()))
        );
    }
}
