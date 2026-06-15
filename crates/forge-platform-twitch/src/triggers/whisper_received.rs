use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};
use regex::Regex;

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
                .get("user")
                .and_then(|u| u.get("login"))
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
            .get("whisper")
            .and_then(|w| w.get("text"))
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
        let user = event.payload.get("user");

        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_display_name = user
            .and_then(|u| u.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_color = user
            .and_then(|u| u.get("color"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let whisper = event.payload.get("whisper");
        let whisper_text = whisper
            .and_then(|w| w.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let whisper_thread_id = event
            .payload
            .get("whisper_thread_id")
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
}
