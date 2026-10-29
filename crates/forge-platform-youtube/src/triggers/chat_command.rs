use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ChatTriggerFamily, EventFilter, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, TriggerConfig, Variant,
    VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::{chat as fields, kinds};

pub(crate) struct ChatCommandDescriptor;

impl TriggerKindDescriptor for ChatCommandDescriptor {
    fn id(&self) -> &str {
        kinds::CHAT_COMMAND
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
            kind_prefix: Some(kinds::CHAT_MESSAGE.to_owned()),
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
                .first_chat_message()
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
                        kind: VariantKind::Array,
                        label: "Command arguments".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::Array(command_args(event).map(Variant::String).collect()),
                )
                .legacy(
                    "user_display_name",
                    "Sender display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy(
                    "channel_id",
                    "Sender channel ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
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

fn command_words(event: &Event) -> impl Iterator<Item = String> + '_ {
    let text = event
        .payload
        .get(fields::MESSAGE_TEXT)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    text.strip_prefix('!')
        .unwrap_or(text)
        .split_whitespace()
        .map(ToOwned::to_owned)
}

fn command_name(event: &Event) -> String {
    command_words(event).next().unwrap_or_default()
}

fn command_args(event: &Event) -> impl Iterator<Item = String> + '_ {
    command_words(event).skip(1)
}

fn author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::AUTHOR))
}

#[cfg(test)]
mod tests {
    use forge_registry::kind_matches_prefix;
    use forge_types::{ChatViewer, SynthesisHint};

    use super::*;

    fn make_config(phrase: &str, case_sensitive: bool) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert("phrase".to_owned(), Variant::String(phrase.to_owned()));
        config.insert("case_sensitive".to_owned(), Variant::Bool(case_sensitive));
        config
    }

    fn chat_line(message: &str) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message",
            serde_json::json!({
                "message_id": "LCC.cmd-1",
                "message_text": message,
                "author": { "display_name": "Viewer", "channel_id": "UCabc" }
            }),
        )
    }

    fn published_chat_line(message: &str) -> Event {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/published_chat_message.json"
        ))
        .unwrap();
        let mut payload = fixture["payload"].clone();
        payload["message_text"] = serde_json::Value::String(message.to_owned());
        Event::new(
            serde_json::from_value(fixture["source"].clone()).unwrap(),
            fixture["kind"].as_str().unwrap(),
            payload,
        )
    }

    #[test]
    fn the_command_trigger_listens_to_and_matches_the_chat_line_the_poller_publishes() {
        let event = published_chat_line("!roll 1d6");
        let filter = ChatCommandDescriptor.event_filter();

        assert_eq!(filter.source, Some(event.source));
        assert!(kind_matches_prefix(
            &event.kind,
            filter.kind_prefix.as_deref().unwrap()
        ));
        assert!(ChatCommandDescriptor.matches_trigger(&make_config("!roll", false), &event));
    }

    #[test]
    fn a_phrase_matches_only_at_the_start_and_only_ignores_case_when_told_to() {
        for (phrase, case_sensitive, message, expected) in [
            ("!roll", false, "!Roll 1d6", true),
            ("!roll", false, "!roll", true),
            ("!roll", true, "!roll 1d6", true),
            ("!roll", true, "!Roll 1d6", false),
            ("!roll", false, "please !roll", false),
            ("!roll", false, "!", false),
            ("!roll", false, "", false),
            ("", false, "!anything", false),
        ] {
            assert_eq!(
                ChatCommandDescriptor
                    .matches_trigger(&make_config(phrase, case_sensitive), &chat_line(message)),
                expected,
                "phrase {phrase:?} case_sensitive {case_sensitive} message {message:?}"
            );
        }
    }

    #[test]
    fn the_command_name_and_arguments_are_split_from_the_message_text() {
        for (message, name, args) in [
            ("!roll 1d6", "roll", vec!["1d6"]),
            ("!roll   1d6 \t adv  ", "roll", vec!["1d6", "adv"]),
            ("!Roll 1D6", "Roll", vec!["1D6"]),
            ("!roll", "roll", vec![]),
            ("!", "", vec![]),
            ("hello there", "hello", vec!["there"]),
        ] {
            let stack = ChatCommandDescriptor.build_arg_stack(&chat_line(message));
            assert_eq!(
                stack.get("command_name"),
                Some(&Variant::String(name.to_owned())),
                "command_name for {message:?}"
            );
            assert_eq!(
                stack.get("args"),
                Some(&Variant::Array(
                    args.into_iter()
                        .map(|arg| Variant::String(arg.to_owned()))
                        .collect()
                )),
                "args for {message:?}"
            );
        }
    }

    #[test]
    fn a_command_publishes_its_sender_its_text_and_the_id_of_its_message() {
        let stack = ChatCommandDescriptor.build_arg_stack(&chat_line("!roll 1d6"));
        for (name, value) in [
            ("user_id", "UCabc"),
            ("user_name", "Viewer"),
            ("message_text", "!roll 1d6"),
            ("message_id", "LCC.cmd-1"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn a_command_reports_whether_it_is_the_viewers_first_chat_message() {
        for first_message in [true, false] {
            let mut event = chat_line("!roll 1d6");
            ChatViewer {
                first_message,
                ..ChatViewer::new("UCabc", "Viewer")
            }
            .attach(&mut event.payload);

            let stack = ChatCommandDescriptor.build_arg_stack(&event);

            assert_eq!(
                stack.get("first_message"),
                Some(&Variant::Bool(first_message))
            );
        }
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
