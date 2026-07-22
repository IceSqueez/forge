use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, VariableSchema, Variant};

use super::chat_arg_stack::{base_chat_args, base_chat_schema};
use crate::payload_fields::chat as fields;

pub(crate) struct ChatCommandDescriptor;

impl TriggerKindDescriptor for ChatCommandDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.command"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat command"
    }

    fn summary(&self) -> &str {
        "Fires when a chat message matches a command phrase"
    }

    fn search_text(&self) -> &str {
        "twitch chat command trigger phrase prefix"
    }

    fn icon_name(&self) -> &str {
        "terminal-2"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
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
            source: Some(EventSource::Twitch),
            kind_prefix: Some("chat.message".to_owned()),
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

        let message = event
            .payload
            .get(fields::MESSAGE)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if case_sensitive {
            message.starts_with(phrase)
        } else {
            message.to_lowercase().starts_with(&phrase.to_lowercase())
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        base_chat_args(event)
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some(base_chat_schema())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_config(phrase: &str, case_sensitive: bool) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert("phrase".to_owned(), Variant::String(phrase.to_owned()));
        config.insert("case_sensitive".to_owned(), Variant::Bool(case_sensitive));
        config
    }

    fn chat_event(message: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "channel": "streamer",
                "user": { "login": "viewer", "id": "123", "roles": [] },
                "message": message,
                "badges": [],
                "color": ""
            }),
        )
    }

    #[test]
    fn condition_display_uses_phrase() {
        let mut cfg = TriggerConfig::new();
        cfg.insert("phrase".to_owned(), Variant::String("!quote".to_owned()));
        assert_eq!(ChatCommandDescriptor.condition_display(&cfg), "\"!quote\"");
    }

    #[test]
    fn condition_display_empty_phrase_returns_any() {
        let mut cfg = TriggerConfig::new();
        cfg.insert("phrase".to_owned(), Variant::String(String::new()));
        assert_eq!(ChatCommandDescriptor.condition_display(&cfg), "any");
    }

    #[test]
    fn matches_case_insensitive_prefix() {
        let cfg = make_config("!quote", false);
        let event = chat_event("!Quote something");
        assert!(ChatCommandDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn matches_exact_prefix_case_sensitive() {
        let cfg = make_config("!quote", true);
        assert!(ChatCommandDescriptor.matches_trigger(&cfg, &chat_event("!quote arg")));
        assert!(!ChatCommandDescriptor.matches_trigger(&cfg, &chat_event("!Quote arg")));
    }

    #[test]
    fn does_not_match_non_prefix() {
        let cfg = make_config("!quote", false);
        assert!(!ChatCommandDescriptor.matches_trigger(&cfg, &chat_event("hello !quote")));
    }

    #[test]
    fn empty_phrase_never_matches() {
        let cfg = make_config("", false);
        assert!(!ChatCommandDescriptor.matches_trigger(&cfg, &chat_event("!anything")));
    }

    #[test]
    fn build_arg_stack_includes_base_chat_args() {
        let stack = ChatCommandDescriptor.build_arg_stack(&chat_event("!quote hello"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("!quote hello".to_owned()))
        );
    }
}
