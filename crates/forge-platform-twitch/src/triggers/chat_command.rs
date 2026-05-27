use forge_events::{Event, EventSource};
use forge_registry::{EventFilter, FormField, TriggerCategory, TriggerKindDescriptor};
use forge_types::{ArgStack, Trigger, TriggerConfig, Variant};

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

    fn matches_trigger(&self, trigger: &Trigger, event: &Event) -> bool {
        let phrase = trigger
            .config
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

        let case_sensitive = trigger
            .config
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
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if case_sensitive {
            message.starts_with(phrase)
        } else {
            message.to_lowercase().starts_with(&phrase.to_lowercase())
        }
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

        ArgStack::new()
            .set("message_text".to_owned(), Variant::String(message_text))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("channel".to_owned(), Variant::String(channel))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::{ActionId, TriggerId};

    fn make_trigger(phrase: &str, case_sensitive: bool) -> Trigger {
        let mut config = TriggerConfig::new();
        config.insert("phrase".to_owned(), Variant::String(phrase.to_owned()));
        config.insert("case_sensitive".to_owned(), Variant::Bool(case_sensitive));
        Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "twitch.chat.command".to_owned(),
            config,
        }
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
    fn id_is_stable() {
        assert_eq!(ChatCommandDescriptor.id(), "twitch.chat.command");
    }

    #[test]
    fn default_config_has_phrase_and_case_sensitive() {
        let cfg = ChatCommandDescriptor.default_config();
        assert!(matches!(cfg.get("phrase"), Some(Variant::String(_))));
        assert!(matches!(
            cfg.get("case_sensitive"),
            Some(Variant::Bool(false))
        ));
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
        let trigger = make_trigger("!quote", false);
        let event = chat_event("!Quote something");
        assert!(ChatCommandDescriptor.matches_trigger(&trigger, &event));
    }

    #[test]
    fn matches_exact_prefix_case_sensitive() {
        let trigger = make_trigger("!quote", true);
        assert!(ChatCommandDescriptor.matches_trigger(&trigger, &chat_event("!quote arg")));
        assert!(!ChatCommandDescriptor.matches_trigger(&trigger, &chat_event("!Quote arg")));
    }

    #[test]
    fn does_not_match_non_prefix() {
        let trigger = make_trigger("!quote", false);
        assert!(!ChatCommandDescriptor.matches_trigger(&trigger, &chat_event("hello !quote")));
    }

    #[test]
    fn empty_phrase_never_matches() {
        let trigger = make_trigger("", false);
        assert!(!ChatCommandDescriptor.matches_trigger(&trigger, &chat_event("!anything")));
    }

    #[test]
    fn build_arg_stack_extracts_message_and_user() {
        let event = chat_event("!quote hello");
        let stack = ChatCommandDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("!quote hello".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer".to_owned()))
        );
        assert_eq!(
            stack.get("channel"),
            Some(&Variant::String("streamer".to_owned()))
        );
    }
}
