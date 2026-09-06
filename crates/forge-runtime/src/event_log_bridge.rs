use std::fmt::{self, Write as _};
use std::sync::{Arc, OnceLock};

use forge_events::Event;
use forge_types::redaction::RedactedText;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use crate::bus::EventBus;

/// Filterable on its own, so a reproduction can raise the event trail without raising every crate.
const TARGET: &str = "forge::event";

const MAX_DEPTH: usize = 6;
const MAX_ENTRIES: usize = 12;
const DIGEST_BYTES: usize = 6;

/// A value longer than this, or one carrying whitespace, is prose wearing an identifier's key name.
const MAX_TOKEN_LEN: usize = 64;

const HANDLE_KEYS: &[&str] = &[
    "author",
    "display_name",
    "handle",
    "login",
    "nick",
    "user_login",
    "user_name",
    "username",
];

pub fn spawn_event_log_bridge(bus: Arc<EventBus>) {
    tokio::spawn(run(bus));
}

async fn run(bus: Arc<EventBus>) {
    let mut rx = bus.subscribe().into_receiver();
    loop {
        match rx.recv().await {
            Ok(event) => emit(&event),
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                tracing::warn!(
                    target: TARGET,
                    missed,
                    "event trail has a gap; the bridge fell behind the bus"
                );
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// The projection is a field expression, so it never runs unless the subscriber wants the record.
fn emit(event: &Event) {
    tracing::debug!(
        target: TARGET,
        kind = event.kind.as_str(),
        id = %event.id,
        caused_by = event.caused_by.map(tracing::field::display),
        source = ?event.source,
        replay = event.replay,
        payload = %Projection(&event.payload),
    );
}

fn salt() -> &'static [u8; 32] {
    /// Fresh per run and never persisted, exported or logged: a platform's handle space is small
    /// enough that a digest under a known salt is reversible by enumeration.
    static SALT: OnceLock<[u8; 32]> = OnceLock::new();
    SALT.get_or_init(rand::random)
}

fn identity_digest(handle: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt());
    hasher.update(handle.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(DIGEST_BYTES * 2);
    for byte in digest.iter().take(DIGEST_BYTES) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn is_token(value: &str) -> bool {
    value.len() <= MAX_TOKEN_LEN && !value.chars().any(char::is_whitespace)
}

fn is_opaque_id_key(key: &str) -> bool {
    key == "id" || key.ends_with("_id")
}

fn is_handle_key(key: &str) -> bool {
    HANDLE_KEYS.contains(&key)
}

/// The one opted-in string: the payload convention defines `reason` as a stable snake_case token,
/// with human prose living in a sibling `detail` the projection still redacts.
fn is_disclosed_key(key: &str) -> bool {
    key == "reason"
}

struct Projection<'a>(&'a Value);

impl fmt::Display for Projection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_value(f, self.0, None, 0)
    }
}

fn write_value(
    f: &mut fmt::Formatter<'_>,
    value: &Value,
    key: Option<&str>,
    depth: usize,
) -> fmt::Result {
    match value {
        Value::Null => f.write_str("null"),
        Value::Bool(b) => write!(f, "{b}"),
        Value::Number(n) => write!(f, "{n}"),
        Value::String(s) => write_string(f, s, key),
        Value::Array(items) => write_array(f, items, depth),
        Value::Object(map) => write_object(f, map, depth),
    }
}

fn write_string(f: &mut fmt::Formatter<'_>, value: &str, key: Option<&str>) -> fmt::Result {
    match key {
        Some(k) if (is_opaque_id_key(k) || is_disclosed_key(k)) && is_token(value) => {
            f.write_str(value)
        }
        Some(k) if is_handle_key(k) => write!(f, "<redacted digest={}>", identity_digest(value)),
        _ => write!(f, "{:?}", RedactedText::new(value)),
    }
}

fn write_array(f: &mut fmt::Formatter<'_>, items: &[Value], depth: usize) -> fmt::Result {
    if depth >= MAX_DEPTH {
        return write!(f, "[{} items]", items.len());
    }
    f.write_str("[")?;
    for (position, item) in items.iter().take(MAX_ENTRIES).enumerate() {
        if position > 0 {
            f.write_str(", ")?;
        }
        write_value(f, item, None, depth + 1)?;
    }
    if let Some(elided) = items.len().checked_sub(MAX_ENTRIES).filter(|n| *n > 0) {
        write!(f, ", +{elided} more")?;
    }
    f.write_str("]")
}

fn write_object(f: &mut fmt::Formatter<'_>, map: &Map<String, Value>, depth: usize) -> fmt::Result {
    if depth >= MAX_DEPTH {
        return write!(f, "{{{} fields}}", map.len());
    }
    let entries: Vec<(&String, &Value)> = match depth {
        0 => map.iter().collect(),
        _ => identity_entries(map).unwrap_or_else(|| map.iter().collect()),
    };
    f.write_str("{")?;
    for (position, (key, value)) in entries.iter().take(MAX_ENTRIES).enumerate() {
        if position > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{key}=")?;
        write_value(f, value, Some(key), depth + 1)?;
    }
    if let Some(elided) = entries.len().checked_sub(MAX_ENTRIES).filter(|n| *n > 0) {
        write!(f, ", +{elided} more")?;
    }
    f.write_str("}")
}

/// An object naming a person collapses to that name's opaque id alone, or - where the source ships
/// none - to a digest of the handle; every sibling field of an identity is dropped unrendered.
fn identity_entries(map: &Map<String, Value>) -> Option<Vec<(&String, &Value)>> {
    let has_handle = map
        .iter()
        .any(|(key, value)| is_handle_key(key) && value.is_string());
    if !has_handle {
        return None;
    }
    let ids: Vec<(&String, &Value)> = map
        .iter()
        .filter(|(key, value)| is_opaque_id_key(key) && !value.is_null())
        .collect();
    if !ids.is_empty() {
        return Some(ids);
    }
    Some(
        map.iter()
            .filter(|(key, value)| is_handle_key(key) && value.is_string())
            .collect(),
    )
}
