use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ChatTriggerFamily, EventFilter, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::chat as fields;

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
            .get(fields::MESSAGE_TEXT)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if case_sensitive {
            message.starts_with(phrase)
        } else {
            message.to_lowercase().starts_with(&phrase.to_lowercase())
        }
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), author_identity)
                .message_text(|event| payload_read::text(event, fields::MESSAGE_TEXT))
                .event_specific(
                    DeclaredVariable {
                        name: "command_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Command name".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::COMMAND_NAME)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "args".to_owned(),
                        kind: VariantKind::Array,
                        label: "Command arguments".to_owned(),
                        synthesis: None,
                    },
                    |event| payload_read::text_list(event, fields::ARGS),
                )
                .legacy(
                    DeclaredVariable {
                        name: "user_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(author_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(author_identity(event).id),
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

fn author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::AUTHOR))
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
                "command_name": "roll",
                "args": ["1d6"],
                "author": { "display_name": "Viewer", "channel_id": "UCabc" }
            }),
        )
    }

    #[test]
    fn a_phrase_matches_only_at_the_start_and_only_ignores_case_when_told_to() {
        for (phrase, case_sensitive, message, expected) in [
            ("!roll", false, "!Roll 1d6", true),
            ("!roll", false, "!roll", true),
            ("!roll", true, "!roll 1d6", true),
            ("!roll", true, "!Roll 1d6", false),
            ("!roll", false, "please !roll", false),
            ("", false, "!anything", false),
        ] {
            assert_eq!(
                ChatCommandDescriptor.matches_trigger(
                    &make_config(phrase, case_sensitive),
                    &command_event(message)
                ),
                expected,
                "phrase {phrase:?} case_sensitive {case_sensitive} message {message:?}"
            );
        }
    }

    #[test]
    fn a_command_publishes_its_sender_its_name_and_its_arguments() {
        let stack = ChatCommandDescriptor.build_arg_stack(&command_event("!roll 1d6"));
        for (name, value) in [
            ("user_id", "UCabc"),
            ("user_name", "Viewer"),
            ("message_text", "!roll 1d6"),
            ("command_name", "roll"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("args"),
            Some(&Variant::Array(vec![Variant::String("1d6".to_owned())]))
        );
    }

    #[test]
    fn the_legacy_sender_names_still_carry_what_their_canonical_twins_carry() {
        let stack = ChatCommandDescriptor.build_arg_stack(&command_event("!roll 1d6"));
        assert_eq!(stack.get("user_display_name"), stack.get("user_name"));
        assert_eq!(stack.get("channel_id"), stack.get("user_id"));
        assert_eq!(
            stack.get("channel_id"),
            Some(&Variant::String("UCabc".to_owned()))
        );
    }
}
