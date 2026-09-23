use forge_events::Event;
use forge_registry::{ActorBlock, ActorIdentity, LoginSlot};
use forge_types::{ActorRole, PlatformId};

use crate::payload_fields::entity as entity_fields;

pub(super) const fn kick_actor(role: ActorRole) -> ActorBlock {
    ActorBlock {
        role,
        platform: PlatformId::Kick,
        login: LoginSlot::Declared,
    }
}

pub(super) fn identity(entity: Option<&serde_json::Value>) -> ActorIdentity {
    identity_keyed(entity, entity_fields::ID, entity_fields::USERNAME)
}

pub(super) fn identity_keyed(
    entity: Option<&serde_json::Value>,
    id_key: &str,
    login_key: &str,
) -> ActorIdentity {
    ActorIdentity {
        id: numeric_id(entity, id_key),
        display_name: nested_text(entity, entity_fields::DISPLAY_NAME),
        login: Some(nested_text(entity, login_key)),
    }
}

pub(super) fn text(event: &Event, key: &str) -> String {
    string_at(event.payload.get(key))
}

pub(super) fn nested_text(parent: Option<&serde_json::Value>, key: &str) -> String {
    string_at(parent.and_then(|value| value.get(key)))
}

pub(super) fn numeric_id(parent: Option<&serde_json::Value>, key: &str) -> String {
    parent
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_u64)
        .map_or_else(String::new, |id| id.to_string())
}

pub(super) fn number(event: &Event, key: &str) -> i64 {
    event
        .payload
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default()
}

pub(super) fn flag(event: &Event, key: &str) -> bool {
    event
        .payload
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_default()
}

pub(super) fn login_of(identity: ActorIdentity) -> String {
    identity.login.unwrap_or_default()
}

fn string_at(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
