use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};
use regex::Regex;

use crate::payload_fields::whisper as whisper_fields;

pub(crate) struct WhisperReceivedDescriptor;

impl TriggerKindDescriptor for WhisperReceivedDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.whisper"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Whisper received"
    }

    fn summary(&self) -> &str {
        "Fires when the bot account receives a whisper"
    }

    fn search_text(&self) -> &str {
        "twitch whisper dm direct message private received bot chat"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "from_user",
                label: "From user (login)",
                placeholder: "any",
            },
            FormField::Text {
                key: "match_text",
                label: "Match text",
                placeholder: "leave empty to match all",
            },
            FormField::Toggle {
                key: "match_is_regex",
                label: "Use regex",
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let from = config
            .get("from_user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let text = config
            .get("match_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        match (from.is_empty(), text.is_empty()) {
            (true, true) => "any whisper".to_owned(),
            (false, true) => format!("from {from}"),
            (true, false) => format!("text matches \"{text}\""),
            (false, false) => format!("from {from}, text matches \"{text}\""),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("user.whisper.message".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let from_user = config
            .get("from_user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase();

        if !from_user.is_empty() {
            let sender_login = event
                .payload
                .get(whisper_fields::USER)
                .and_then(|u| u.get(whisper_fields::USER_LOGIN))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            if sender_login != from_user {
                return false;
            }
        }

        let match_text = config
            .get("match_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        if match_text.is_empty() {
            return true;
        }

        let whisper_text = event
            .payload
            .get(whisper_fields::WHISPER)
            .and_then(|w| w.get(whisper_fields::WHISPER_TEXT))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let is_regex = config
            .get("match_is_regex")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if is_regex {
            Regex::new(match_text)
                .map(|re| re.is_match(whisper_text))
                .unwrap_or(false)
        } else {
            whisper_text.contains(match_text)
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event.payload.get(whisper_fields::USER);

        let user_id = user
            .and_then(|u| u.get(whisper_fields::USER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get(whisper_fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_display_name = user
            .and_then(|u| u.get(whisper_fields::USER_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_color = user
            .and_then(|u| u.get(whisper_fields::USER_COLOR))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let whisper = event.payload.get(whisper_fields::WHISPER);
        let whisper_text = whisper
            .and_then(|w| w.get(whisper_fields::WHISPER_TEXT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let whisper_thread_id = event
            .payload
            .get(whisper_fields::WHISPER_THREAD_ID)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("whisper.text".to_owned(), Variant::String(whisper_text))
            .set(
                "whisper.thread_id".to_owned(),
                Variant::String(whisper_thread_id),
            )
            .set("user.id".to_owned(), Variant::String(user_id))
            .set("user.login".to_owned(), Variant::String(user_login))
            .set(
                "user.display_name".to_owned(),
                Variant::String(user_display_name),
            )
            .set("user.color".to_owned(), Variant::String(user_color))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "whisper.text".to_owned(),
                        kind: VariantKind::String,
                        label: "Whisper text".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    DeclaredVariable {
                        name: "whisper.thread_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Whisper thread ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "user.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    DeclaredVariable {
                        name: "user.color".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender name color".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, Variant)]) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        for (key, value) in pairs {
            cfg.insert((*key).to_owned(), value.clone());
        }
        cfg
    }

    fn whisper_event(from_login: &str, text: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "user.whisper.message",
            serde_json::json!({
                "user": {
                    "id": "42",
                    "login": from_login,
                    "display_name": "BobX",
                    "color": "#00FF00"
                },
                "whisper": { "text": text },
                "whisper_thread_id": "thread-7"
            }),
        )
    }

    #[test]
    fn empty_config_matches_any_whisper() {
        let cfg = TriggerConfig::new();
        assert!(WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "hi")));
    }

    #[test]
    fn from_user_filter_is_case_insensitive() {
        let cfg = config(&[("from_user", Variant::String("BobX".to_owned()))]);
        assert!(WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "hi")));
    }

    #[test]
    fn from_user_filter_rejects_other_sender() {
        let cfg = config(&[("from_user", Variant::String("bobx".to_owned()))]);
        assert!(!WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("alice", "hi")));
    }

    #[test]
    fn match_text_substring_present_matches() {
        let cfg = config(&[("match_text", Variant::String("secret".to_owned()))]);
        assert!(
            WhisperReceivedDescriptor
                .matches_trigger(&cfg, &whisper_event("bobx", "the secret code"))
        );
    }

    #[test]
    fn match_text_substring_absent_rejects() {
        let cfg = config(&[("match_text", Variant::String("secret".to_owned()))]);
        assert!(
            !WhisperReceivedDescriptor
                .matches_trigger(&cfg, &whisper_event("bobx", "nothing here"))
        );
    }

    #[test]
    fn regex_match_text_matches_when_pattern_hits() {
        let cfg = config(&[
            ("match_text", Variant::String(r"^!play\s+\d+$".to_owned())),
            ("match_is_regex", Variant::Bool(true)),
        ]);
        assert!(
            WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "!play 42"))
        );
    }

    #[test]
    fn regex_match_text_rejects_when_pattern_misses() {
        let cfg = config(&[
            ("match_text", Variant::String(r"^!play\s+\d+$".to_owned())),
            ("match_is_regex", Variant::Bool(true)),
        ]);
        assert!(
            !WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "!play later"))
        );
    }

    #[test]
    fn invalid_regex_does_not_panic_and_rejects() {
        let cfg = config(&[
            ("match_text", Variant::String("[".to_owned())),
            ("match_is_regex", Variant::Bool(true)),
        ]);
        assert!(
            !WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "[bracket"))
        );
    }

    #[test]
    fn combined_from_user_and_match_text_both_must_pass() {
        let cfg = config(&[
            ("from_user", Variant::String("bobx".to_owned())),
            ("match_text", Variant::String("hello".to_owned())),
        ]);
        assert!(
            WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "well hello"))
        );
        assert!(
            !WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("bobx", "goodbye"))
        );
        assert!(
            !WhisperReceivedDescriptor.matches_trigger(&cfg, &whisper_event("alice", "well hello"))
        );
    }

    #[test]
    fn build_arg_stack_extracts_whisper_and_user_fields() {
        let stack =
            WhisperReceivedDescriptor.build_arg_stack(&whisper_event("bobx", "the message"));
        assert_eq!(
            stack.get("whisper.text"),
            Some(&Variant::String("the message".to_owned()))
        );
        assert_eq!(
            stack.get("whisper.thread_id"),
            Some(&Variant::String("thread-7".to_owned()))
        );
        assert_eq!(
            stack.get("user.id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("user.login"),
            Some(&Variant::String("bobx".to_owned()))
        );
        assert_eq!(
            stack.get("user.display_name"),
            Some(&Variant::String("BobX".to_owned()))
        );
        assert_eq!(
            stack.get("user.color"),
            Some(&Variant::String("#00FF00".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_yields_empty_strings() {
        let event = Event::new(
            EventSource::Twitch,
            "user.whisper.message",
            serde_json::json!({}),
        );
        let stack = WhisperReceivedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("whisper.text"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("user.login"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn event_filter_targets_twitch_whisper_topic() {
        let filter = WhisperReceivedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("user.whisper.message"));
    }
}
