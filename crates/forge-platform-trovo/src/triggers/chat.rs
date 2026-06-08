use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChatDescriptor;

impl TriggerKindDescriptor for ChatDescriptor {
    fn id(&self) -> &str {
        "trovo.chat"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in Trovo live chat"
    }

    fn search_text(&self) -> &str {
        "trovo chat message trigger any incoming live"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Trovo)
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
            source: Some(EventSource::Trovo),
            kind_prefix: Some("trovo.chat".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_standard_arg_stack(event)
    }
}

pub(crate) fn build_standard_arg_stack(event: &Event) -> ArgStack {
    let content = str_field(&event.payload, "content");
    let nick_name = str_field(&event.payload, "nick_name");
    let user_name = str_field(&event.payload, "user_name");
    let sender_id = str_field(&event.payload, "sender_id");

    ArgStack::new()
        .set("content".to_owned(), Variant::String(content))
        .set("nick_name".to_owned(), Variant::String(nick_name))
        .set("user_name".to_owned(), Variant::String(user_name))
        .set("sender_id".to_owned(), Variant::String(sender_id))
}

pub(crate) fn str_field(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;

    fn chat_event() -> Event {
        Event::new(
            EventSource::Trovo,
            "trovo.chat",
            serde_json::json!({
                "content": "hello world",
                "nick_name": "StreamerDisplay",
                "user_name": "streamer_login",
                "sender_id": "uid_123"
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(ChatDescriptor.matches_trigger(&TriggerConfig::new(), &chat_event()));
    }

    #[test]
    fn build_arg_stack_extracts_all_fields() {
        let stack = ChatDescriptor.build_arg_stack(&chat_event());
        assert_eq!(
            stack.get("content"),
            Some(&Variant::String("hello world".to_owned()))
        );
        assert_eq!(
            stack.get("nick_name"),
            Some(&Variant::String("StreamerDisplay".to_owned()))
        );
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("streamer_login".to_owned()))
        );
        assert_eq!(
            stack.get("sender_id"),
            Some(&Variant::String("uid_123".to_owned()))
        );
    }
}
