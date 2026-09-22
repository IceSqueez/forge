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
