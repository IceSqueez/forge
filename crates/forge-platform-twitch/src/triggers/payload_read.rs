use forge_events::Event;
use forge_registry::{ActorBlock, ActorIdentity, LoginSlot};
use forge_types::{ActorRole, PlatformId, Variant};

pub(super) const fn twitch_actor(role: ActorRole) -> ActorBlock {
    ActorBlock {
        role,
        platform: PlatformId::Twitch,
        login: LoginSlot::Declared,
    }
}

pub(super) fn identity(
    actor: Option<&serde_json::Value>,
    id: &str,
    login: &str,
    display_name: &str,
) -> ActorIdentity {
    ActorIdentity {
        id: actor_text(actor, id),
        display_name: actor_text(actor, display_name),
        login: Some(actor_text(actor, login)),
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
    actor_text(event.payload.get(object), key)
}

pub(super) fn number(event: &Event, key: &str) -> i64 {
    event
        .payload
        .get(key)
        .and_then(|value| value.as_i64())
        .unwrap_or_default()
}

pub(super) fn nested_number(event: &Event, object: &str, key: &str) -> i64 {
    event
        .payload
        .get(object)
        .and_then(|nested| nested.get(key))
        .and_then(|value| value.as_i64())
        .unwrap_or_default()
}

pub(super) fn nested_flag(event: &Event, object: &str, key: &str) -> bool {
    event
        .payload
        .get(object)
        .and_then(|nested| nested.get(key))
        .and_then(|value| value.as_bool())
        .unwrap_or_default()
}

pub(super) fn text_list(event: &Event, key: &str) -> Variant {
    collect_text(event.payload.get(key))
}

pub(super) fn nested_text_list(event: &Event, object: &str, key: &str) -> Variant {
    collect_text(event.payload.get(object).and_then(|nested| nested.get(key)))
}

pub(super) fn flag(event: &Event, key: &str) -> bool {
    event
        .payload
        .get(key)
        .and_then(|value| value.as_bool())
        .unwrap_or_default()
}

fn collect_text(values: Option<&serde_json::Value>) -> Variant {
    Variant::Array(
        values
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

fn actor_text(actor: Option<&serde_json::Value>, key: &str) -> String {
    actor
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}
