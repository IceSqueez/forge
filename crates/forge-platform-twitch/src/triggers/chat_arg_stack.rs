use forge_events::Event;
use forge_registry::{ActorBlock, ActorIdentity, LoginSlot, TriggerVariables};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, Variant, VariantKind};

use crate::payload_fields::chat as fields;

const CHATTER: ActorBlock = ActorBlock {
    role: ActorRole::Principal,
    platform: PlatformId::Twitch,
    login: LoginSlot::Declared,
};

pub(super) fn base_chat_variables() -> TriggerVariables {
    TriggerVariables::new()
        .actor(CHATTER, chatter_identity)
        .message_text(chat_message_text)
        .event_specific(
            DeclaredVariable {
                name: "channel".to_owned(),
                kind: VariantKind::String,
                label: "Channel login".to_owned(),
                synthesis: None,
            },
            |event| Variant::String(text_field(event, fields::CHANNEL)),
        )
        .event_specific(
            DeclaredVariable {
                name: "user_color".to_owned(),
                kind: VariantKind::String,
                label: "Username color".to_owned(),
                synthesis: None,
            },
            |event| Variant::String(text_field(event, fields::COLOR)),
        )
}

fn chatter_identity(event: &Event) -> ActorIdentity {
    ActorIdentity {
        id: user_field(event, fields::USER_ID),
        display_name: user_field(event, fields::USER_DISPLAY_NAME),
        login: Some(user_field(event, fields::USER_LOGIN)),
    }
}

fn chat_message_text(event: &Event) -> String {
    text_field(event, fields::MESSAGE)
}

fn text_field(event: &Event, key: &str) -> String {
    event
        .payload
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn user_field(event: &Event, key: &str) -> String {
    event
        .payload
        .get(fields::USER)
        .and_then(|user| user.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::EventSource;
    use forge_types::ArgStack;

    use super::*;

    fn chat_payload(display_name: serde_json::Value) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({
                "channel": "streamer",
                "user": {
                    "login": "loyalfan",
                    "id": "222",
                    "display_name": display_name,
                    "roles": [],
                },
                "message": "!quote hello there",
                "badges": [],
                "color": "#FF0000",
            }),
        )
    }

    fn chat_args(display_name: serde_json::Value) -> ArgStack {
        base_chat_variables().arg_stack(&chat_payload(display_name))
    }

    #[test]
    fn the_shared_chat_block_publishes_the_chatter_identity_the_channel_and_the_line() {
        let stack = chat_args(serde_json::json!("LoyalFan"));
        let expected = [
            ("user_id", "222"),
            ("user_name", "LoyalFan"),
            ("user_login", "loyalfan"),
            ("user_platform", "twitch"),
            ("message_text", "!quote hello there"),
            ("channel", "streamer"),
            ("user_color", "#FF0000"),
        ];
        for (name, value) in expected {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
        assert_eq!(stack.snapshot().len(), expected.len());
    }

    #[test]
    fn the_shared_chat_block_declares_the_canonical_names_before_the_channel_and_the_color() {
        let declared: Vec<String> = base_chat_variables()
            .schema()
            .variables
            .into_iter()
            .map(|variable| variable.name)
            .collect();
        assert_eq!(
            declared,
            [
                "user_id",
                "user_name",
                "user_login",
                "user_platform",
                "message_text",
                "channel",
                "user_color",
            ]
        );
    }

    #[test]
    fn a_chatter_without_a_display_name_is_shown_under_the_login() {
        for absent in [serde_json::json!(""), serde_json::json!(null)] {
            let stack = chat_args(absent.clone());
            assert_eq!(
                stack.get("user_name"),
                Some(&Variant::String("loyalfan".to_owned())),
                "display name {absent}"
            );
        }
    }

    #[test]
    fn a_chat_payload_missing_every_field_publishes_empty_strings_rather_than_nothing() {
        let event = Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({}),
        );
        let stack = base_chat_variables().arg_stack(&event);
        for name in [
            "user_id",
            "user_name",
            "user_login",
            "message_text",
            "channel",
            "user_color",
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
        assert_eq!(
            stack.get("user_platform"),
            Some(&Variant::String("twitch".to_owned()))
        );
    }
}
