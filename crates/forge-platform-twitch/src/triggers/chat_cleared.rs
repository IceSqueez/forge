use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::chat_mod as chat_mod_fields;

pub(crate) struct ChatClearedDescriptor;

impl TriggerKindDescriptor for ChatClearedDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.cleared"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat Cleared"
    }

    fn summary(&self) -> &str {
        "Fires when all messages are cleared from the chat room"
    }

    fn search_text(&self) -> &str {
        "twitch chat clear purge moderation all messages"
    }

    fn icon_name(&self) -> &str {
        "eraser"
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
            kind_prefix: Some("twitch.channel.chat.clear".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let broadcaster_login = event
            .payload
            .get(chat_mod_fields::BROADCASTER)
            .and_then(|b| b.get(chat_mod_fields::BROADCASTER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let broadcaster_id = event
            .payload
            .get(chat_mod_fields::BROADCASTER)
            .and_then(|b| b.get(chat_mod_fields::BROADCASTER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "broadcaster_login".to_owned(),
                Variant::String(broadcaster_login),
            )
            .set("broadcaster_id".to_owned(), Variant::String(broadcaster_id))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "broadcaster_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcaster login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "broadcaster_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcaster ID".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chat_clear_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.chat.clear",
            serde_json::json!({
                "broadcaster": { "id": "100", "login": "host_channel" }
            }),
        )
    }

    #[test]
    fn event_filter_gates_on_chat_clear_kind_prefix() {
        let filter = ChatClearedDescriptor.event_filter();
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.chat.clear")
        );
        assert_eq!(filter.source, Some(EventSource::Twitch));
    }

    #[test]
    fn build_arg_stack_maps_broadcaster_from_nested_payload() {
        let stack = ChatClearedDescriptor.build_arg_stack(&chat_clear_event());
        assert_eq!(
            stack.get("broadcaster_login"),
            Some(&Variant::String("host_channel".to_owned()))
        );
        assert_eq!(
            stack.get("broadcaster_id"),
            Some(&Variant::String("100".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_to_empty_strings_when_broadcaster_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.chat.clear",
            serde_json::json!({}),
        );
        let stack = ChatClearedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("broadcaster_login"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("broadcaster_id"),
            Some(&Variant::String(String::new()))
        );
    }
}
