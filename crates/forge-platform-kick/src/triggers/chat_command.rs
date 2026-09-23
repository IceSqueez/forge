use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ChatTriggerFamily, EventFilter, FormField, KindPlatformContract,
    TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::chat::{sender_color, sender_identity};
use super::payload_read::{self, kick_actor};
use crate::payload_fields::chat as fields;

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

        let content = payload_read::text(event, fields::CONTENT);

        if case_sensitive {
            content.starts_with(phrase)
        } else {
            content.to_lowercase().starts_with(&phrase.to_lowercase())
        }
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), sender_identity)
                .message_text(|event| payload_read::text(event, fields::CONTENT))
                .event_specific(
                    DeclaredVariable {
                        name: "message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::MESSAGE_ID)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "color".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender name color".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(sender_color(event)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reply_to_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Replied-to message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::REPLY_TO_MESSAGE_ID)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "command_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Command name".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(command_name(event)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "args".to_owned(),
                        kind: VariantKind::String,
                        label: "Command arguments".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    |event| Variant::String(command_args(event)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "sender_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender user ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(sender_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "username".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(sender_identity(event))),
                )
                .legacy(
                    DeclaredVariable {
                        name: "display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(sender_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "content".to_owned(),
                        kind: VariantKind::String,
                        label: "Message content".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| Variant::String(payload_read::text(event, fields::CONTENT)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Command)
    }
}

fn command_name(event: &Event) -> String {
    payload_read::text(event, fields::CONTENT)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn command_args(event: &Event) -> String {
    let content = payload_read::text(event, fields::CONTENT);
    content
        .trim_start_matches(command_name(event).as_str())
        .trim_start()
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
        cfg.insert("phrase".to_owned(), Variant::String(phrase.to_owned()));
        cfg.insert("case_sensitive".to_owned(), Variant::Bool(case_sensitive));
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
    fn the_command_name_and_the_args_split_at_the_first_whitespace() {
        for (content, name, args) in [
            ("!so @someone hello", "!so", "@someone hello"),
            ("!ping", "!ping", ""),
            ("!ping   ", "!ping", ""),
            ("!ping\tone\ttwo", "!ping", "one\ttwo"),
            ("!привіт світ", "!привіт", "світ"),
            ("", "", ""),
        ] {
            let stack = ChatCommandDescriptor.build_arg_stack(&command_event(content));
            assert_eq!(
                stack.get("command_name"),
                Some(&Variant::String(name.to_owned())),
                "content {content:?}"
            );
            assert_eq!(
                stack.get("args"),
                Some(&Variant::String(args.to_owned())),
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
}
