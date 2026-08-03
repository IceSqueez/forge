use forge_events::{Event, EventSource};
use forge_registry::{
    ChatTriggerFamily, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::{chat as fields, entity};

pub(crate) struct ChatDescriptor;

impl TriggerKindDescriptor for ChatDescriptor {
    fn id(&self) -> &str {
        "kick.chat.message.sent"
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
            kind_prefix: Some("kick.chat.message.sent".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let sender = event.payload.get(fields::SENDER);
        let sender_id = sender
            .and_then(|s| s.get(entity::ID))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let username = str_field_nested(sender, entity::USERNAME);
        let display_name = str_field_nested(sender, entity::DISPLAY_NAME);
        let color = str_field_nested(sender, fields::COLOR);

        let message_id = str_field(&event.payload, fields::MESSAGE_ID);
        let content = str_field(&event.payload, fields::CONTENT);
        let reply_to_id = str_field(&event.payload, fields::REPLY_TO_MESSAGE_ID);

        ArgStack::new()
            .set("message_id".to_owned(), Variant::String(message_id))
            .set("sender_id".to_owned(), Variant::String(sender_id))
            .set("username".to_owned(), Variant::String(username))
            .set("display_name".to_owned(), Variant::String(display_name))
            .set("content".to_owned(), Variant::String(content))
            .set("color".to_owned(), Variant::String(color))
            .set("reply_to_id".to_owned(), Variant::String(reply_to_id))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "message_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Message ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "sender_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender user ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "username".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender username".to_owned(),
                    synthesis: Some(SynthesisHint::Username),
                },
                DeclaredVariable {
                    name: "display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender display name".to_owned(),
                    synthesis: Some(SynthesisHint::DisplayName),
                },
                DeclaredVariable {
                    name: "content".to_owned(),
                    kind: VariantKind::String,
                    label: "Message content".to_owned(),
                    synthesis: Some(SynthesisHint::Message),
                },
                DeclaredVariable {
                    name: "color".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender name color".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "reply_to_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Replied-to message ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Message)
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
            "kick.chat.message.sent",
            serde_json::json!({
                "message_id": "msg-1",
                "content": "hello stream",
                "reply_to_message_id": null,
                "sender": {
                    "id": 42,
                    "username": "viewer_slug",
                    "display_name": "Viewer Display",
                    "color": "#00FF00"
                }
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
            stack.get("message_id"),
            Some(&Variant::String("msg-1".to_owned()))
        );
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
