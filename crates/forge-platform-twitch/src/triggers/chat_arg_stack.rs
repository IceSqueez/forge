use forge_events::Event;
use forge_types::{
    ArgStack, DeclaredVariable, SynthesisHint, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::chat as fields;

pub(super) fn base_chat_args(event: &Event) -> ArgStack {
    let message_text = event
        .payload
        .get(fields::MESSAGE)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let user_login = event
        .payload
        .get(fields::USER)
        .and_then(|u| u.get(fields::USER_LOGIN))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let user_id = event
        .payload
        .get(fields::USER)
        .and_then(|u| u.get(fields::USER_ID))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let channel = event
        .payload
        .get(fields::CHANNEL)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let color = event
        .payload
        .get(fields::COLOR)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    ArgStack::new()
        .set("message_text".to_owned(), Variant::String(message_text))
        .set("user_login".to_owned(), Variant::String(user_login))
        .set("user_id".to_owned(), Variant::String(user_id))
        .set("channel".to_owned(), Variant::String(channel))
        .set("user_color".to_owned(), Variant::String(color))
}

pub(super) fn base_chat_schema() -> VariableSchema {
    VariableSchema {
        variables: vec![
            DeclaredVariable {
                name: "message_text".to_owned(),
                kind: VariantKind::String,
                label: "Chat message text".to_owned(),
                synthesis: Some(SynthesisHint::Message),
            },
            DeclaredVariable {
                name: "user_login".to_owned(),
                kind: VariantKind::String,
                label: "User login".to_owned(),
                synthesis: Some(SynthesisHint::Username),
            },
            DeclaredVariable {
                name: "user_id".to_owned(),
                kind: VariantKind::String,
                label: "User ID".to_owned(),
                synthesis: None,
            },
            DeclaredVariable {
                name: "channel".to_owned(),
                kind: VariantKind::String,
                label: "Channel login".to_owned(),
                synthesis: None,
            },
            DeclaredVariable {
                name: "user_color".to_owned(),
                kind: VariantKind::String,
                label: "Username color".to_owned(),
                synthesis: None,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;

    #[test]
    fn base_chat_args_extracts_all_chat_fields() {
        let event = Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "channel": "streamer",
                "user": { "login": "viewer", "id": "123", "roles": [] },
                "message": "hello",
                "badges": [],
                "color": "#FF0000"
            }),
        );
        let stack = base_chat_args(&event);
        for (key, expected) in [
            ("message_text", "hello"),
            ("user_login", "viewer"),
            ("user_id", "123"),
            ("channel", "streamer"),
            ("user_color", "#FF0000"),
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(expected.to_owned())),
                "wrong value for {key}"
            );
        }
    }

    #[test]
    fn base_chat_args_defaults_missing_fields_to_empty_strings() {
        let event = Event::new(EventSource::Twitch, "chat.message", serde_json::json!({}));
        let stack = base_chat_args(&event);
        for key in [
            "message_text",
            "user_login",
            "user_id",
            "channel",
            "user_color",
        ] {
            assert_eq!(
                stack.get(key),
                Some(&Variant::String(String::new())),
                "expected empty-string default for {key}"
            );
        }
    }
}
