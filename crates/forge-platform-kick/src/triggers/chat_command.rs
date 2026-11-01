use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, COMMAND_CASE_SENSITIVE_KEY, COMMAND_PHRASE_KEY, ChatTriggerFamily,
    EventFilter, FormField, KindPlatformContract, SubActionConfigExt, TriggerCatalogEntry,
    TriggerCategory, TriggerKindDescriptor, TriggerVariables, command_phrase_matches,
};
use forge_types::{PlatformId, TriggerConfig, UserText, Variant};

use super::chat::{chat_line_variables, with_sender_aliases};
use super::payload_read;
use crate::vocabulary::fields::chat as fields;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) struct ChatCommandDescriptor;

impl TriggerCatalogEntry for ChatCommandDescriptor {
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
}

impl TriggerKindDescriptor for ChatCommandDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_COMMAND
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            COMMAND_PHRASE_KEY.to_owned(),
            Variant::String("!command".to_owned()),
        );
        cfg.insert(COMMAND_CASE_SENSITIVE_KEY.to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: COMMAND_PHRASE_KEY,
                label: "Command phrase",
                placeholder: "!command",
            },
            FormField::Toggle {
                key: COMMAND_CASE_SENSITIVE_KEY,
                label: "Case sensitive",
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        forge_registry::chat_command_condition(config)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Kick),
            kind_prefix: Some(event_kinds::CHAT_MESSAGE_SENT.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let phrase = config.str(COMMAND_PHRASE_KEY).unwrap_or("");

        if phrase.is_empty() {
            return false;
        }

        let case_sensitive = config.bool(COMMAND_CASE_SENSITIVE_KEY).unwrap_or(false);

        let content = payload_read::text(event, fields::CONTENT);

        command_phrase_matches(&content, phrase, case_sensitive)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(with_sender_aliases(chat_line_variables().command_line(
            |event| payload_read::text(event, fields::CONTENT),
        )))
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use forge_types::SynthesisHint;
    use serde_json::json;

    use crate::triggers::chat::ChatDescriptor;

    fn command_event(content: &str) -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            json!({
                "message_id": "msg-1",
                "content": content,
                "reply_to_message_id": "parent-99",
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
        cfg.insert(
            COMMAND_PHRASE_KEY.to_owned(),
            Variant::String(phrase.to_owned()),
        );
        cfg.insert(
            COMMAND_CASE_SENSITIVE_KEY.to_owned(),
            Variant::Bool(case_sensitive),
        );
        cfg
    }

    #[test]
    fn the_phrase_admits_only_the_messages_it_prefixes() {
        for (phrase, case_sensitive, content, expected) in [
            ("!roll", false, "!Roll 1d6", true),
            ("!roll", false, "!roll 1d6", true),
            ("!roll", true, "!Roll 1d6", false),
            ("!roll", true, "!roll 1d6", true),
            ("!roll", false, "please !roll for me", false),
            ("!roll", false, "!rollover", false),
            ("!roll", false, "", false),
            ("", false, "!roll 1d6", false),
            ("", true, "", false),
            ("!привіт", false, "!ПРИВІТ світ", true),
        ] {
            assert_eq!(
                ChatCommandDescriptor
                    .matches_trigger(&config(phrase, case_sensitive), &command_event(content)),
                expected,
                "phrase {phrase:?} case_sensitive {case_sensitive} content {content:?}"
            );
        }
    }

    #[test]
    fn the_matcher_reads_the_content_field_and_never_a_message_field() {
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            json!({ "message": "!roll 1d6", "content": "" }),
        );
        assert!(!ChatCommandDescriptor.matches_trigger(&config("!roll", false), &event));
    }

    #[test]
    fn the_command_name_and_the_tail_split_at_the_first_whitespace() {
        for (content, name, tail) in [
            ("!so @someone hello", "so", "@someone hello"),
            ("!ping", "ping", ""),
            ("!ping   ", "ping", ""),
            ("!ping\tone\ttwo", "ping", "one\ttwo"),
            ("!привіт світ", "привіт", "світ"),
            ("", "", ""),
        ] {
            let stack = ChatCommandDescriptor.build_arg_stack(&command_event(content));
            assert_eq!(
                stack.get("command_name"),
                Some(&Variant::String(name.to_owned())),
                "content {content:?}"
            );
            assert_eq!(
                stack.get("command.tail"),
                Some(&Variant::String(tail.to_owned())),
                "content {content:?}"
            );
        }
    }

    #[test]
    fn the_command_trigger_publishes_the_same_chat_context_as_the_plain_message_trigger() {
        let event = command_event("!so @someone hello");
        let plain = ChatDescriptor.build_arg_stack(&event);
        let command = ChatCommandDescriptor.build_arg_stack(&event);
        for (name, value) in plain.snapshot() {
            assert_eq!(
                command.get(&name),
                Some(&value),
                "'{name}' drifted from the plain message trigger"
            );
        }
        assert_eq!(
            command.get("user_login"),
            Some(&Variant::String("viewer_slug".to_owned()))
        );
        assert_eq!(
            command.get("user_name"),
            Some(&Variant::String("Viewer Display".to_owned()))
        );
    }

    #[test]
    fn output_schema_declares_message_text_as_its_first_message_hinted_variable() {
        let schema = ChatCommandDescriptor.output_schema().unwrap();

        let first_message_hinted = schema
            .variables
            .iter()
            .find(|v| v.synthesis == Some(SynthesisHint::Message))
            .map(|v| v.name.as_str());

        assert_eq!(first_message_hinted, Some("message_text"));
    }
}
