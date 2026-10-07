use forge_events::Event;
use forge_types::{ArgStack, ChatPayload, ChatSegment, DeclaredVariable, Variant, VariantKind};

pub const MESSAGE_EMOTES_VARIABLE: &str = "message_emotes";

pub fn message_emotes_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: MESSAGE_EMOTES_VARIABLE.to_owned(),
        kind: VariantKind::Array,
        label: "Emote codes in the message".to_owned(),
        synthesis: None,
    }
}

pub fn chat_emote_codes(event: &Event) -> Vec<String> {
    let Some(envelope) = event.payload.get(ChatPayload::KEY) else {
        return Vec::new();
    };
    let Ok(chat) = serde_json::from_value::<ChatPayload>(envelope.clone()) else {
        return Vec::new();
    };
    chat.segments
        .into_iter()
        .filter_map(|segment| match segment {
            ChatSegment::Emote { name, .. } if !name.trim().is_empty() => Some(name),
            _ => None,
        })
        .collect()
}

pub fn message_emote_codes(args: &ArgStack) -> Vec<String> {
    args.get(MESSAGE_EMOTES_VARIABLE)
        .and_then(Variant::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Variant::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_events::EventSource;
    use serde_json::json;

    use super::*;

    fn chat_event(chat: serde_json::Value) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            json!({ ChatPayload::KEY: chat }),
        )
    }

    fn chat_with(segments: serde_json::Value) -> serde_json::Value {
        json!({
            "platform_msg_id": "m-1",
            "author": "NovaFox",
            "author_color": null,
            "segments": segments,
            "badges": [],
            "is_event": false,
            "event_detail": null,
        })
    }

    #[test]
    fn chat_emote_codes_keep_only_named_emote_segments_in_message_order() {
        let event = chat_event(chat_with(json!([
            { "type": "emote", "id": "425618", "name": "LUL" },
            { "type": "text", "text": " that was " },
            { "type": "mention", "username": "aurora" },
            { "type": "emote", "id": "Cheer", "name": "Cheer100" },
            { "type": "emote", "id": "1", "name": "" },
            { "type": "emote", "id": "2", "name": "   " },
            { "type": "emote", "id": "425618", "name": "LUL" },
        ])));

        assert_eq!(chat_emote_codes(&event), ["LUL", "Cheer100", "LUL"]);
    }

    #[test]
    fn chat_emote_codes_are_empty_for_an_absent_or_unreadable_chat_envelope() {
        for (label, payload) in [
            ("no envelope", json!({ "message": "LUL" })),
            (
                "envelope is not an object",
                json!({ ChatPayload::KEY: "LUL" }),
            ),
            (
                "envelope missing its segments",
                json!({ ChatPayload::KEY: { "platform_msg_id": "m-1", "author": "NovaFox" } }),
            ),
            (
                "no segments",
                json!({ ChatPayload::KEY: chat_with(json!([])) }),
            ),
        ] {
            let event = Event::new(EventSource::Twitch, "twitch.channel.chat.message", payload);

            assert!(chat_emote_codes(&event).is_empty(), "{label}");
        }
    }

    #[test]
    fn message_emote_codes_read_the_string_items_of_the_variable() {
        let codes = |value: Option<Variant>| {
            let stack = match value {
                Some(value) => ArgStack::new().set(MESSAGE_EMOTES_VARIABLE.to_owned(), value),
                None => ArgStack::new(),
            };
            message_emote_codes(&stack)
        };
        let string = |s: &str| Variant::String(s.to_owned());
        for (label, value, expected) in [
            (
                "strings",
                Some(Variant::Array(vec![string("LUL"), string("Kappa")])),
                vec!["LUL", "Kappa"],
            ),
            (
                "mixed item types",
                Some(Variant::Array(vec![
                    Variant::Int(7),
                    string("LUL"),
                    Variant::Bool(true),
                    Variant::Array(vec![string("Kappa")]),
                ])),
                vec!["LUL"],
            ),
            ("not an array", Some(string("LUL")), vec![]),
            ("missing", None, vec![]),
        ] {
            assert_eq!(codes(value), expected, "{label}");
        }
    }
}
