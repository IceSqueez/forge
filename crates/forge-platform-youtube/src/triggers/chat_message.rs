use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::{chat as fields, entity};

pub(crate) struct ChatMessageDescriptor;

impl TriggerKindDescriptor for ChatMessageDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.message"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube chat message trigger any incoming live"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let message_text = event
            .payload
            .get(fields::MESSAGE_TEXT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let author = event.payload.get(fields::AUTHOR);
        let user_display_name = author
            .and_then(|a| a.get(entity::DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let channel_id = author
            .and_then(|a| a.get(entity::CHANNEL_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("message_text".to_owned(), Variant::String(message_text))
            .set(
                "user_display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set("channel_id".to_owned(), Variant::String(channel_id))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "message_text".to_owned(),
                    kind: VariantKind::String,
                    label: "Message text".to_owned(),
                    synthesis: Some(SynthesisHint::Message),
                },
                DeclaredVariable {
                    name: "user_display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender display name".to_owned(),
                    synthesis: Some(SynthesisHint::DisplayName),
                },
                DeclaredVariable {
                    name: "channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Sender channel ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chat_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message",
            serde_json::json!({
                "message_text": "hello world",
                "author": { "display_name": "Viewer One", "channel_id": "UCxyz" }
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(ChatMessageDescriptor.matches_trigger(&TriggerConfig::new(), &chat_event()));
    }

    #[test]
    fn build_arg_stack_extracts_fields() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event());
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hello world".to_owned()))
        );
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("Viewer One".to_owned()))
        );
        assert_eq!(
            stack.get("channel_id"),
            Some(&Variant::String("UCxyz".to_owned()))
        );
    }
}
