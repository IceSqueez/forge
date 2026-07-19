use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

pub(crate) struct ChatCommandDescriptor;

impl TriggerKindDescriptor for ChatCommandDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.command"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat command"
    }

    fn summary(&self) -> &str {
        "Fires when a YouTube chat message matches a command phrase"
    }

    fn search_text(&self) -> &str {
        "youtube chat command trigger phrase prefix live"
    }

    fn icon_name(&self) -> &str {
        "terminal-2"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.chat.command".to_owned()),
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
            .get("message_text")
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
            .get("message_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let command_name = event
            .payload
            .get("command_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let args = event
            .payload
            .get("args")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_display_name = event
            .payload
            .get("user_display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let channel_id = event
            .payload
            .get("channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("message_text".to_owned(), Variant::String(message_text))
            .set("command_name".to_owned(), Variant::String(command_name))
            .set("args".to_owned(), Variant::String(args))
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
                    name: "command_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Command name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "args".to_owned(),
                    kind: VariantKind::String,
                    label: "Command arguments".to_owned(),
                    synthesis: None,
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

    fn make_config(phrase: &str, case_sensitive: bool) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert("phrase".to_owned(), Variant::String(phrase.to_owned()));
        config.insert("case_sensitive".to_owned(), Variant::Bool(case_sensitive));
        config
    }

    fn command_event(message: &str) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.command",
            serde_json::json!({
                "message_text": message,
                "command_name": "!roll",
                "args": "1d6",
                "user_display_name": "Viewer",
                "channel_id": "UCabc"
            }),
        )
    }

    #[test]
    fn matches_case_insensitive_prefix() {
        let cfg = make_config("!roll", false);
        assert!(ChatCommandDescriptor.matches_trigger(&cfg, &command_event("!Roll 1d6")));
    }

    #[test]
    fn does_not_match_empty_phrase() {
        let cfg = make_config("", false);
        assert!(!ChatCommandDescriptor.matches_trigger(&cfg, &command_event("!anything")));
    }

    #[test]
    fn build_arg_stack_extracts_command_fields() {
        let stack = ChatCommandDescriptor.build_arg_stack(&command_event("!roll 1d6"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("!roll 1d6".to_owned()))
        );
        assert_eq!(
            stack.get("command_name"),
            Some(&Variant::String("!roll".to_owned()))
        );
        assert_eq!(stack.get("args"), Some(&Variant::String("1d6".to_owned())));
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("Viewer".to_owned()))
        );
    }
}
