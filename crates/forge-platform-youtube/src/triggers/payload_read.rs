use forge_events::Event;
use forge_registry::{ActorBlock, ActorIdentity, LoginSlot};
use forge_types::{ActorRole, PlatformId, Variant};

use crate::payload_fields::entity as entity_fields;

pub(super) const fn youtube_actor(role: ActorRole) -> ActorBlock {
    ActorBlock {
        role,
        platform: PlatformId::YouTube,
        login: LoginSlot::PlatformHasNone,
    }
}

pub(super) fn identity(entity: Option<&serde_json::Value>) -> ActorIdentity {
    ActorIdentity {
        id: entity_text(entity, entity_fields::CHANNEL_ID),
        display_name: entity_text(entity, entity_fields::DISPLAY_NAME),
        login: None,
    }
}

pub(super) fn text(event: &Event, key: &str) -> String {
    event
        .payload
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

pub(super) fn nested_text(event: &Event, object: &str, key: &str) -> String {
    entity_text(event.payload.get(object), key)
}

pub(super) fn number(event: &Event, key: &str) -> i64 {
    event
        .payload
        .get(key)
        .and_then(|value| value.as_i64())
        .unwrap_or_default()
}

pub(super) fn text_list(event: &Event, key: &str) -> Variant {
    Variant::Array(
        event
            .payload
            .get(key)
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(|item| Variant::String(item.to_owned()))
                    .collect()
            })
            .unwrap_or_default(),
    )
}

fn entity_text(entity: Option<&serde_json::Value>, key: &str) -> String {
    entity
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use forge_events::EventSource;
    use serde_json::json;

    use super::*;

    fn event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::YouTube, "youtube.chat.message", payload)
    }

    #[test]
    fn a_text_field_the_wire_omits_or_sends_unquoted_reads_as_the_empty_string() {
        for payload in [
            json!({}),
            json!({ "message_text": null }),
            json!({ "message_text": 7 }),
            json!({ "message_text": ["hello"] }),
            json!({ "message_text": { "text": "hello" } }),
        ] {
            assert_eq!(
                text(&event(payload.clone()), "message_text"),
                String::new(),
                "payload {payload}"
            );
        }
    }

    #[test]
    fn a_number_field_the_wire_omits_or_sends_as_text_reads_as_zero() {
        for payload in [
            json!({}),
            json!({ "count": null }),
            json!({ "count": "5" }),
            json!({ "count": 5.5 }),
        ] {
            assert_eq!(
                number(&event(payload.clone()), "count"),
                0,
                "payload {payload}"
            );
        }
        assert_eq!(number(&event(json!({ "count": -3 })), "count"), -3);
    }

    #[test]
    fn a_nested_field_reads_through_its_object_and_survives_a_missing_one() {
        assert_eq!(
            nested_text(
                &event(json!({ "title": { "new": "Live" } })),
                "title",
                "new"
            ),
            "Live"
        );
        for payload in [json!({}), json!({ "title": "Live" }), json!({ "title": 7 })] {
            assert_eq!(
                nested_text(&event(payload.clone()), "title", "new"),
                String::new(),
                "payload {payload}"
            );
        }
    }

    #[test]
    fn a_text_list_keeps_only_the_strings_the_wire_actually_sent() {
        assert_eq!(
            text_list(&event(json!({ "args": ["1d6", 7, null, "adv"] })), "args"),
            Variant::Array(vec![
                Variant::String("1d6".to_owned()),
                Variant::String("adv".to_owned()),
            ])
        );
        for payload in [json!({}), json!({ "args": "1d6" }), json!({ "args": null })] {
            assert_eq!(
                text_list(&event(payload.clone()), "args"),
                Variant::Array(vec![]),
                "payload {payload}"
            );
        }
    }

    #[test]
    fn an_identity_reads_the_channel_id_and_display_name_apart_and_never_invents_a_login() {
        let full = identity(Some(&json!({
            "channel_id": "UCxyz",
            "display_name": "Viewer One",
        })));
        assert_eq!(
            full,
            ActorIdentity {
                id: "UCxyz".to_owned(),
                display_name: "Viewer One".to_owned(),
                login: None,
            }
        );

        for entity in [
            None,
            Some(json!({})),
            Some(json!({ "channel_id": null, "display_name": null })),
        ] {
            assert_eq!(
                identity(entity.as_ref()),
                ActorIdentity {
                    id: String::new(),
                    display_name: String::new(),
                    login: None,
                },
                "entity {entity:?}"
            );
        }
    }
}
