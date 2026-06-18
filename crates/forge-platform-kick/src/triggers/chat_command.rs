use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

use super::chat::str_field;

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
            kind_prefix: Some("kick.chat.message".to_owned()),
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

        let content = str_field(&event.payload, "content");

        if case_sensitive {
            content.starts_with(phrase)
        } else {
            content.to_lowercase().starts_with(&phrase.to_lowercase())
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let sender = event.payload.get("sender");
        let sender_id = sender
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let username = sender
            .and_then(|p| p.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let display_name = sender
            .and_then(|p| p.get("slug"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
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

        let command_name = content.split_whitespace().next().unwrap_or("").to_owned();
        let args = content
            .trim_start_matches(command_name.as_str())
            .trim_start()
            .to_owned();

        ArgStack::new()
            .set("sender_id".to_owned(), Variant::String(sender_id))
            .set("username".to_owned(), Variant::String(username))
            .set("display_name".to_owned(), Variant::String(display_name))
            .set("content".to_owned(), Variant::String(content))
            .set("color".to_owned(), Variant::String(color))
            .set("reply_to_id".to_owned(), Variant::String(reply_to_id))
            .set("command_name".to_owned(), Variant::String(command_name))
            .set("args".to_owned(), Variant::String(args))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn command_event(content: &str) -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message",
            serde_json::json!({
                "id": "msg-1",
                "chatroom_id": 100,
                "content": content,
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
        // The descriptor must read `content`; a payload whose command lives
        // under `message` (the wrong key) must not match.
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message",
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
    fn arg_stack_reply_to_id_reads_metadata_original_message() {
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message",
            serde_json::json!({
                "content": "!ping",
                "sender": { "id": 42 },
                "metadata": { "original_message": { "id": "parent-99" } }
            }),
        );
        let stack = ChatCommandDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("reply_to_id"),
            Some(&Variant::String("parent-99".to_owned()))
        );
    }
}
