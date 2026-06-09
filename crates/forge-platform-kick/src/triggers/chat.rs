use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChatDescriptor;

impl TriggerKindDescriptor for ChatDescriptor {
    fn id(&self) -> &str {
        "kick.chat"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in Kick live chat"
    }

    fn search_text(&self) -> &str {
        "kick chat message trigger any incoming live"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
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
            kind_prefix: Some("kick.chat".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let sender = event.payload.get("sender");
        let sender_id = sender
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let username = str_field_nested(sender, "username");
        let slug = str_field_nested(sender, "slug");
        let color = sender
            .and_then(|s| s.get("identity"))
            .and_then(|i| i.get("color"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let content = str_field(&event.payload, "content");
        let reply_to_id = event
            .payload
            .get("metadata")
            .and_then(|m| m.get("original_message"))
            .and_then(|o| o.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("sender_id".to_owned(), Variant::String(sender_id))
            .set("username".to_owned(), Variant::String(username))
            .set("display_name".to_owned(), Variant::String(slug))
            .set("content".to_owned(), Variant::String(content))
            .set("color".to_owned(), Variant::String(color))
            .set("reply_to_id".to_owned(), Variant::String(reply_to_id))
    }
}

pub(crate) fn str_field(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

fn str_field_nested(parent: Option<&serde_json::Value>, key: &str) -> String {
    parent
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chat_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat",
            serde_json::json!({
                "id": "msg-1",
                "chatroom_id": 100,
                "content": "hello stream",
                "type": "message",
                "sender": {
                    "id": 42,
                    "username": "viewer_slug",
                    "slug": "Viewer Display",
                    "identity": { "color": "#00FF00", "badges": [] }
                },
                "metadata": null
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(ChatDescriptor.matches_trigger(&TriggerConfig::new(), &chat_event()));
    }

    #[test]
    fn build_arg_stack_extracts_fields() {
        let stack = ChatDescriptor.build_arg_stack(&chat_event());
        assert_eq!(
            stack.get("sender_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("viewer_slug".to_owned()))
        );
        assert_eq!(
            stack.get("content"),
            Some(&Variant::String("hello stream".to_owned()))
        );
        assert_eq!(
            stack.get("color"),
            Some(&Variant::String("#00FF00".to_owned()))
        );
        assert_eq!(
            stack.get("reply_to_id"),
            Some(&Variant::String(String::new()))
        );
    }
}
