use forge_events::Event;
use forge_types::{ChatViewer, DeclaredVariable, TriggerConfig, Variant, VariantKind};

use crate::form::FormField;

pub const FIRST_CHATTERS_ONLY: &str = "first_chatters_only";
pub const FIRST_MESSAGE_VARIABLE: &str = "first_message";

const ANY_MESSAGE_CONDITION: &str = "any";
const FIRST_CHATTERS_CONDITION: &str = "first-time chatters";

pub fn first_chatters_only_field() -> FormField {
    FormField::Toggle {
        key: FIRST_CHATTERS_ONLY,
        label: "Only first-time chatters",
    }
}

pub fn first_message_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: FIRST_MESSAGE_VARIABLE.to_owned(),
        kind: VariantKind::Bool,
        label: "First message forge has seen from this viewer".to_owned(),
        synthesis: None,
    }
}

pub fn is_first_chat_message(event: &Event) -> bool {
    ChatViewer::read(&event.payload).is_some_and(|viewer| viewer.first_message)
}

pub fn first_chatters_only(config: &TriggerConfig) -> bool {
    config
        .get(FIRST_CHATTERS_ONLY)
        .and_then(Variant::as_bool)
        .unwrap_or(false)
}

pub fn admits_chatter(config: &TriggerConfig, event: &Event) -> bool {
    !first_chatters_only(config) || is_first_chat_message(event)
}

pub fn chat_message_condition(config: &TriggerConfig) -> String {
    if first_chatters_only(config) {
        FIRST_CHATTERS_CONDITION.to_owned()
    } else {
        ANY_MESSAGE_CONDITION.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use forge_events::EventSource;

    use super::*;

    fn chat_line(envelope: Option<bool>) -> Event {
        let mut payload = serde_json::json!({ "message": "hi" });
        if let Some(first_message) = envelope {
            ChatViewer {
                first_message,
                ..ChatViewer::new("42", "viewer")
            }
            .attach(&mut payload);
        }
        Event::new(EventSource::Twitch, "twitch.channel.chat.message", payload)
    }

    fn filter(value: Option<Variant>) -> TriggerConfig {
        value
            .map(|v| TriggerConfig::from([(FIRST_CHATTERS_ONLY.to_owned(), v)]))
            .unwrap_or_default()
    }

    #[test]
    fn the_filter_admits_every_chatter_unless_it_is_on_and_then_only_a_first_message() {
        let on = || filter(Some(Variant::Bool(true)));
        let off = || filter(Some(Variant::Bool(false)));
        for (config, envelope, admitted) in [
            (filter(None), Some(false), true),
            (filter(None), None, true),
            (off(), Some(false), true),
            (off(), Some(true), true),
            (on(), Some(true), true),
            (on(), Some(false), false),
            (on(), None, false),
            (
                filter(Some(Variant::String("true".to_owned()))),
                Some(false),
                true,
            ),
        ] {
            assert_eq!(
                admits_chatter(&config, &chat_line(envelope)),
                admitted,
                "config {config:?} envelope {envelope:?}"
            );
        }
    }

    #[test]
    fn the_condition_reads_first_time_chatters_only_while_the_filter_is_on() {
        for (config, expected) in [
            (filter(Some(Variant::Bool(true))), FIRST_CHATTERS_CONDITION),
            (filter(Some(Variant::Bool(false))), ANY_MESSAGE_CONDITION),
            (filter(None), ANY_MESSAGE_CONDITION),
        ] {
            assert_eq!(chat_message_condition(&config), expected, "{config:?}");
        }
    }
}
