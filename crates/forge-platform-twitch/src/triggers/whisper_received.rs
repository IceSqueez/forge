use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
    joined_condition,
};
use forge_types::{
    DeclaredVariable, MessageRef, PlatformId, TriggerConfig, UserText, Variant, VariantKind,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tracing::warn;

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::whisper as whisper_fields;
use crate::vocabulary::{event_kinds, messages, trigger_ids};

const COMPILED_PATTERN_CACHE_LIMIT: usize = 64;

pub(crate) struct WhisperReceivedDescriptor;

fn compiled_pattern(pattern: &str) -> Option<Regex> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Regex>>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some(compiled) = cache.get(pattern) {
        return compiled.clone();
    }
    if cache.len() >= COMPILED_PATTERN_CACHE_LIMIT {
        cache.clear();
    }
    let compiled = match Regex::new(pattern) {
        Ok(compiled) => Some(compiled),
        Err(e) => {
            warn!(error = %e, "whisper trigger pattern does not compile; the trigger never matches");
            None
        }
    };
    cache.insert(pattern.to_owned(), compiled.clone());
    compiled
}

impl TriggerKindDescriptor for WhisperReceivedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_WHISPER
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

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        let filter_text = |key: &str| {
            config
                .get(key)
                .and_then(Variant::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
        };
        let parts: Vec<MessageRef> = filter_text("from_user")
            .map(messages::condition_whisper_from)
            .into_iter()
            .chain(filter_text("match_text").map(messages::condition_whisper_text))
            .collect();
        if parts.is_empty() {
            messages::CONDITION_ANY_WHISPER.into()
        } else {
            joined_condition(parts)
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::USER_WHISPER_MESSAGE.to_owned()),
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
            compiled_pattern(match_text).is_some_and(|re| re.is_match(whisper_text))
        } else {
            whisper_text.contains(match_text)
        }
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), sender_identity)
                .message_text(whisper_text)
                .event_specific(
                    DeclaredVariable {
                        name: "whisper.thread_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Whisper thread ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::text(
                            event,
                            whisper_fields::WHISPER_THREAD_ID,
                        ))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "user.color".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender name color".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            whisper_fields::USER,
                            whisper_fields::USER_COLOR,
                        ))
                    },
                )
                .legacy(
                    "whisper.text",
                    "Whisper text",
                    CanonicalVariable::MessageText,
                )
                .legacy(
                    "user.id",
                    "Sender ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                )
                .legacy(
                    "user.login",
                    "Sender login",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                )
                .legacy(
                    "user.display_name",
                    "Sender display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn sender_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(whisper_fields::USER),
        whisper_fields::USER_ID,
        whisper_fields::USER_LOGIN,
        whisper_fields::USER_DISPLAY_NAME,
    )
}

fn whisper_text(event: &Event) -> String {
    payload_read::nested_text(event, whisper_fields::WHISPER, whisper_fields::WHISPER_TEXT)
}

#[cfg(test)]
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

    fn regex_config(pattern: &str) -> TriggerConfig {
        config(&[
            ("match_text", Variant::String(pattern.to_owned())),
            ("match_is_regex", Variant::Bool(true)),
        ])
    }

    #[test]
    fn an_invalid_pattern_warns_once_across_repeated_matches() {
        let cfg = regex_config("(unclosed-group-only-this-test-uses");
        let event = whisper_event("bobx", "anything");

        let (matches, lines) = crate::log_capture::capture_blocking(tracing::Level::WARN, async {
            [
                WhisperReceivedDescriptor.matches_trigger(&cfg, &event),
                WhisperReceivedDescriptor.matches_trigger(&cfg, &event),
            ]
        });

        let warnings = lines
            .iter()
            .filter(|line| line.message().contains("does not compile"))
            .count();
        assert_eq!(matches, [false, false]);
        assert_eq!(warnings, 1);
    }

    #[test]
    fn repeated_patterns_keep_their_own_verdicts() {
        let event = whisper_event("bobx", "hello");
        let hits = regex_config("^hel+o$");
        let misses = regex_config("^bye$");

        let verdicts = [&hits, &misses, &hits, &misses]
            .map(|cfg| WhisperReceivedDescriptor.matches_trigger(cfg, &event));

        assert_eq!(verdicts, [true, false, true, false]);
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
    fn a_sender_without_a_display_name_is_shown_under_the_login_in_the_legacy_display_name() {
        let event = Event::new(
            EventSource::Twitch,
            "user.whisper.message",
            serde_json::json!({
                "user": { "id": "42", "login": "bobx", "display_name": "" },
                "whisper": { "text": "psst" }
            }),
        );
        let stack = WhisperReceivedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("user.display_name"),
            Some(&Variant::String("bobx".to_owned()))
        );
    }
}
