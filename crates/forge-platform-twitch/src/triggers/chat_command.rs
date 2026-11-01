use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, COMMAND_CASE_SENSITIVE_KEY, COMMAND_PHRASE_KEY, ChatTriggerFamily, FormField,
    SubActionConfigExt, TriggerCategory, TriggerVariables, command_phrase_matches,
};
use forge_types::{TriggerConfig, UserText, Variant};

use super::chat_arg_stack::{base_chat_variables, chat_message_text};
use crate::vocabulary::fields::chat as fields;

fn default_config() -> TriggerConfig {
    let mut cfg = TriggerConfig::new();
    cfg.insert(
        COMMAND_PHRASE_KEY.to_owned(),
        Variant::String("!command".to_owned()),
    );
    cfg.insert(COMMAND_CASE_SENSITIVE_KEY.to_owned(), Variant::Bool(false));
    cfg
}

fn config_fields() -> Vec<FormField> {
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

fn condition_display(config: &TriggerConfig) -> UserText {
    forge_registry::chat_command_condition(config)
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    let phrase = config.str(COMMAND_PHRASE_KEY).unwrap_or("");

    if phrase.is_empty() {
        return false;
    }

    let case_sensitive = config.bool(COMMAND_CASE_SENSITIVE_KEY).unwrap_or(false);

    let message = event
        .payload
        .get(fields::MESSAGE)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    command_phrase_matches(message, phrase, case_sensitive)
}

fn variables() -> Option<TriggerVariables> {
    Some(base_chat_variables().command_line(chat_message_text))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHAT_COMMAND,
    category: TriggerCategory::Chat,
    label: "Chat command",
    summary: "Fires when a chat message matches a command phrase",
    search_text: "twitch chat command trigger phrase prefix",
    icon: "terminal-2",
    kind: event_kinds::CHANNEL_CHAT_MESSAGE,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    default_config,
    config_fields,
    condition_display,
    chat_family: Some(ChatTriggerFamily::Command),
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn make_config(phrase: &str, case_sensitive: bool) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        config.insert(
            COMMAND_PHRASE_KEY.to_owned(),
            Variant::String(phrase.to_owned()),
        );
        config.insert(
            COMMAND_CASE_SENSITIVE_KEY.to_owned(),
            Variant::Bool(case_sensitive),
        );
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
    fn condition_display_names_the_configured_phrase() {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            COMMAND_PHRASE_KEY.to_owned(),
            Variant::String("!quote".to_owned()),
        );
        assert_ne!(
            SPEC.condition_display(&cfg),
            forge_registry::any_condition()
        );
    }

    #[test]
    fn matches_case_insensitive_prefix() {
        let cfg = make_config("!quote", false);
        let event = chat_event("!Quote something");
        assert!(SPEC.matches_trigger(&cfg, &event));
    }

    #[test]
    fn matches_exact_prefix_case_sensitive() {
        let cfg = make_config("!quote", true);
        assert!(SPEC.matches_trigger(&cfg, &chat_event("!quote arg")));
        assert!(!SPEC.matches_trigger(&cfg, &chat_event("!Quote arg")));
    }

    #[test]
    fn does_not_match_non_prefix() {
        let cfg = make_config("!quote", false);
        assert!(!SPEC.matches_trigger(&cfg, &chat_event("hello !quote")));
    }

    #[test]
    fn a_phrase_does_not_match_a_longer_command_sharing_its_prefix() {
        let cfg = make_config("!so", false);
        assert!(!SPEC.matches_trigger(&cfg, &chat_event("!sound on")));
    }

    #[test]
    fn empty_phrase_never_matches() {
        let cfg = make_config("", false);
        assert!(!SPEC.matches_trigger(&cfg, &chat_event("!anything")));
    }

    #[test]
    fn output_schema_declares_message_text_as_its_first_message_hinted_variable() {
        let schema = SPEC.output_schema().unwrap();

        let first_message_hinted = schema
            .variables
            .iter()
            .find(|v| v.synthesis == Some(forge_types::SynthesisHint::Message))
            .map(|v| v.name.as_str());

        assert_eq!(first_message_hinted, Some("message_text"));
    }

    #[test]
    fn build_arg_stack_includes_base_chat_args() {
        let stack = SPEC.build_arg_stack(&chat_event("!quote hello"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("!quote hello".to_owned()))
        );
    }
}
