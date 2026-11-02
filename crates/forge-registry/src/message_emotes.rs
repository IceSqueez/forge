use std::collections::BTreeMap;

use forge_events::Event;
use forge_types::{ArgStack, ChatPayload, ChatSegment, DeclaredVariable, Variant, VariantKind};

pub(crate) const MESSAGE_EMOTES_VARIABLE: &str = "message_emotes";
pub(crate) const MESSAGE_EMOTE_IDS_VARIABLE: &str = "message_emote_ids";

pub(crate) fn message_emotes_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: MESSAGE_EMOTES_VARIABLE.to_owned(),
        kind: VariantKind::Array,
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

pub(crate) fn message_emote_ids_declaration() -> DeclaredVariable {
    DeclaredVariable {
        name: MESSAGE_EMOTE_IDS_VARIABLE.to_owned(),
        kind: VariantKind::Object,
        synthesis: None,
    }
}

fn is_cheer(id: &str, name: &str) -> bool {
    name.get(..id.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(id))
        .and_then(|_| name.get(id.len()..))
        .is_some_and(|amount| !amount.is_empty() && amount.chars().all(|c| c.is_ascii_digit()))
}

pub fn chat_emote_ids(event: &Event) -> BTreeMap<String, String> {
    let Some(envelope) = event.payload.get(ChatPayload::KEY) else {
        return BTreeMap::new();
    };
    let Ok(chat) = serde_json::from_value::<ChatPayload>(envelope.clone()) else {
        return BTreeMap::new();
    };
    chat.segments
        .into_iter()
        .filter_map(|segment| match segment {
            ChatSegment::Emote { id, name }
                if !id.trim().is_empty() && !name.trim().is_empty() && !is_cheer(&id, &name) =>
            {
                Some((name, id))
            }
            _ => None,
        })
        .collect()
}

pub fn message_emote_ids(args: &ArgStack) -> BTreeMap<String, String> {
    args.get(MESSAGE_EMOTE_IDS_VARIABLE)
        .and_then(Variant::as_object)
        .map(|ids| {
            ids.iter()
                .filter_map(|(name, id)| Some((name.clone(), id.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
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
    fn chat_emote_ids_map_each_native_emote_name_to_its_id_and_skip_cheermotes() {
        let event = chat_event(chat_with(json!([
            { "type": "emote", "id": "425618", "name": "LUL" },
            { "type": "text", "text": " and " },
            { "type": "emote", "id": "emotesv2_dcd06b30a5c24f6eb871e8f5edbd44f7", "name": "DinoDance" },
            { "type": "emote", "id": "Cheer", "name": "Cheer100" },
            { "type": "emote", "id": "cheer", "name": "Cheer500" },
            { "type": "emote", "id": "", "name": "Blank" },
            { "type": "emote", "id": "3", "name": "  " },
        ])));

        assert_eq!(
            chat_emote_ids(&event),
            BTreeMap::from([
                (
                    "DinoDance".to_owned(),
                    "emotesv2_dcd06b30a5c24f6eb871e8f5edbd44f7".to_owned()
                ),
                ("LUL".to_owned(), "425618".to_owned()),
            ])
        );
    }

    #[test]
    fn an_emote_named_after_its_id_without_a_bit_amount_is_not_a_cheermote() {
        let event = chat_event(chat_with(json!([
            { "type": "emote", "id": "Kappa", "name": "Kappa" },
            { "type": "emote", "id": "Cheer", "name": "CheerHype" },
        ])));

        assert_eq!(
            chat_emote_ids(&event).into_keys().collect::<Vec<_>>(),
            ["CheerHype", "Kappa"]
        );
    }

    #[test]
    fn message_emote_ids_read_only_the_string_ids_of_the_variable() {
        let ids = |value: Option<Variant>| {
            let stack = match value {
                Some(value) => ArgStack::new().set(MESSAGE_EMOTE_IDS_VARIABLE.to_owned(), value),
                None => ArgStack::new(),
            };
            message_emote_ids(&stack)
        };
        let mixed = Variant::Object(BTreeMap::from([
            ("LUL".to_owned(), Variant::String("425618".to_owned())),
            ("Kappa".to_owned(), Variant::Int(25)),
        ]));
        for (label, value, expected) in [
            (
                "mixed id types",
                Some(mixed),
                BTreeMap::from([("LUL".to_owned(), "425618".to_owned())]),
            ),
            (
                "not an object",
                Some(Variant::String("LUL".to_owned())),
                BTreeMap::new(),
            ),
            ("missing", None, BTreeMap::new()),
        ] {
            assert_eq!(ids(value), expected, "{label}");
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
