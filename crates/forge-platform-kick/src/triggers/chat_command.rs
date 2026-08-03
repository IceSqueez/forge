use forge_events::{Event, EventSource};
use forge_registry::{
    ChatTriggerFamily, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use super::chat::str_field;
use crate::payload_fields::{chat as fields, entity};

pub(crate) struct ChatCommandDescriptor;

impl TriggerKindDescriptor for ChatCommandDescriptor {
    fn id(&self) -> &str {
        "kick.chat.command"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat command"
    }

    fn summary(&self) -> &str {
        "Fires when a Kick chat message starts with a command phrase"
    }

    fn search_text(&self) -> &str {
        "kick chat command trigger phrase prefix"
    }

    fn icon_name(&self) -> &str {
        "terminal-2"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("phrase".to_owned(), Variant::String("!command".to_owned()));
        cfg.insert("case_sensitive".to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "phrase",
                label: "Command phrase",
                placeholder: "!command",
            },
            FormField::Toggle {
                key: "case_sensitive",
                label: "Case sensitive",
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        config
            .get("phrase")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .filter(|s| !s.is_empty())
            .map(|p| format!("\"{}\"", p))
            .unwrap_or_else(|| "any".to_owned())
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.chat.message.sent".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let phrase = config
            .get("phrase")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("");

        if phrase.is_empty() {
            return false;
        }

        let case_sensitive = config
            .get("case_sensitive")
            .and_then(|v| {
                if let Variant::Bool(b) = v {
                    Some(*b)
                } else {
                    None
                }
            })
            .unwrap_or(false);

        let content = str_field(&event.payload, fields::CONTENT);

        if case_sensitive {
            content.starts_with(phrase)
        } else {
            content.to_lowercase().starts_with(&phrase.to_lowercase())
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let sender = event.payload.get(fields::SENDER);
        let sender_id = sender
            .and_then(|s| s.get(entity::ID))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let username = sender
            .and_then(|p| p.get(entity::USERNAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let display_name = sender
            .and_then(|p| p.get(entity::DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let color = sender
            .and_then(|s| s.get(fields::COLOR))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let message_id = str_field(&event.payload, fields::MESSAGE_ID);
        let content = str_field(&event.payload, fields::CONTENT);
        let reply_to_id = str_field(&event.payload, fields::REPLY_TO_MESSAGE_ID);

        let command_name = content.split_whitespace().next().unwrap_or("").to_owned();
        let args = content
            .trim_start_matches(command_name.as_str())
            .trim_start()
            .to_owned();

        ArgStack::new()
            .set("message_id".to_owned(), Variant::String(message_id))
            .set("sender_id".to_owned(), Variant::String(sender_id))
            .set("username".to_owned(), Variant::String(username))
            .set("display_name".to_owned(), Variant::String(display_name))
            .set("content".to_owned(), Variant::String(content))
            .set("color".to_owned(), Variant::String(color))
            .set("reply_to_id".to_owned(), Variant::String(reply_to_id))
            .set("command_name".to_owned(), Variant::String(command_name))
            .set("args".to_owned(), Variant::String(args))
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
                DeclaredVariable {
                    name: "command_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Command name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "args".to_owned(),
                    kind: VariantKind::String,
                    label: "Command arguments".to_owned(),
                    synthesis: Some(SynthesisHint::Message),
                },
            ],
        })
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Command)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn command_event(content: &str) -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::json!({
                "message_id": "msg-1",
                "content": content,
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

    fn config(phrase: &str, case_sensitive: bool) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("phrase".to_owned(), Variant::String(phrase.to_owned()));
        cfg.insert("case_sensitive".to_owned(), Variant::Bool(case_sensitive));
        cfg
    }

    #[test]
    fn matches_prefix_case_insensitively_by_default() {
        let event = command_event("!Roll 1d6");
        assert!(ChatCommandDescriptor.matches_trigger(&config("!roll", false), &event));
    }

    #[test]
    fn case_sensitive_rejects_differing_case() {
        let event = command_event("!Roll 1d6");
        assert!(!ChatCommandDescriptor.matches_trigger(&config("!roll", true), &event));
    }

    #[test]
    fn case_sensitive_matches_exact_case() {
        let event = command_event("!roll 1d6");
        assert!(ChatCommandDescriptor.matches_trigger(&config("!roll", true), &event));
    }

    #[test]
    fn empty_phrase_never_matches() {
        let event = command_event("!roll 1d6");
        assert!(!ChatCommandDescriptor.matches_trigger(&config("", false), &event));
    }

    #[test]
    fn content_not_starting_with_phrase_does_not_match() {
        let event = command_event("please !roll for me");
        assert!(!ChatCommandDescriptor.matches_trigger(&config("!roll", false), &event));
    }

    #[test]
    fn matches_reads_content_field_not_message_field() {
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::json!({ "message": "!roll 1d6", "content": "" }),
        );
        assert!(!ChatCommandDescriptor.matches_trigger(&config("!roll", false), &event));
    }

    #[test]
    fn arg_stack_splits_command_from_multi_word_args() {
        let stack = ChatCommandDescriptor.build_arg_stack(&command_event("!so @someone hello"));
        assert_eq!(
            stack.get("command_name"),
            Some(&Variant::String("!so".to_owned()))
        );
        assert_eq!(
            stack.get("args"),
            Some(&Variant::String("@someone hello".to_owned()))
        );
    }

    #[test]
    fn arg_stack_yields_empty_args_when_command_has_none() {
        let stack = ChatCommandDescriptor.build_arg_stack(&command_event("!ping"));
        assert_eq!(
            stack.get("command_name"),
            Some(&Variant::String("!ping".to_owned()))
        );
        assert_eq!(stack.get("args"), Some(&Variant::String(String::new())));
    }

    #[test]
    fn arg_stack_extracts_chat_context_fields() {
        let stack = ChatCommandDescriptor.build_arg_stack(&command_event("!ping"));
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
            stack.get("display_name"),
            Some(&Variant::String("Viewer Display".to_owned()))
        );
        assert_eq!(
            stack.get("content"),
            Some(&Variant::String("!ping".to_owned()))
        );
        assert_eq!(
            stack.get("color"),
            Some(&Variant::String("#00FF00".to_owned()))
        );
    }

    #[test]
    fn arg_stack_carries_normalized_reply_to_message_id() {
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::json!({
                "content": "!ping",
                "sender": { "id": 42 },
                "reply_to_message_id": "parent-99"
            }),
        );
        let stack = ChatCommandDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("reply_to_id"),
            Some(&Variant::String("parent-99".to_owned()))
        );
    }
}
