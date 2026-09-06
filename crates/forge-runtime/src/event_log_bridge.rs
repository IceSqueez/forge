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

pub(crate) fn identity_digest(handle: &str) -> String {
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
        // `reason` is deliberately NOT disclosed: on moderation events it carries a moderator's
        // free text about a viewer, and a one-word slur passes any token-shape test.
        Some(k) if is_opaque_id_key(k) && is_token(value) => f.write_str(value),
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

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(value: Value) -> String {
        Projection(&value).to_string()
    }

    fn single(key: &str, value: Value) -> String {
        project(json!({ key: value }))
    }

    /// Pulls the hex out of the one `<redacted digest=...>` a rendering is expected to carry.
    fn digest_in(rendered: &str) -> String {
        let (_, tail) = rendered
            .split_once("<redacted digest=")
            .unwrap_or_else(|| panic!("no digest in {rendered:?}"));
        let (hex, _) = tail
            .split_once('>')
            .unwrap_or_else(|| panic!("unterminated digest in {rendered:?}"));
        hex.to_owned()
    }

    fn nest(levels: usize, leaf: Value) -> Value {
        let mut value = leaf;
        for _ in 0..levels {
            value = json!({ "a": value });
        }
        value
    }

    /// Why: the projection is default-deny over *strings* only. Scalars carry no viewer text, and
    /// blanking them would strip the counts and flags a reproduction is read for.
    #[test]
    fn envelope_scalars_render_verbatim() {
        assert_eq!(
            project(json!({
                "absent": Value::Null,
                "count": 12,
                "enabled": true,
                "muted": false,
                "ratio": 1.5,
                "shift": -3,
            })),
            "{absent=null, count=12, enabled=true, muted=false, ratio=1.5, shift=-3}"
        );
    }

    #[test]
    fn only_id_keys_disclose_and_only_when_the_value_is_a_token() {
        let rows: Vec<(&str, String, String)> = vec![
            ("id", "abc-123".into(), "{id=abc-123}".into()),
            ("user_id", "42".into(), "{user_id=42}".into()),
            // Boundary: MAX_TOKEN_LEN is inclusive, one over degrades.
            (
                "message_id",
                "a".repeat(MAX_TOKEN_LEN),
                format!("{{message_id={}}}", "a".repeat(MAX_TOKEN_LEN)),
            ),
            (
                "message_id",
                "a".repeat(MAX_TOKEN_LEN + 1),
                format!("{{message_id=<redacted len={}>}}", MAX_TOKEN_LEN + 1),
            ),
            // Any whitespace disqualifies the disclosure, not just a space.
            ("id", "a\tb".into(), "{id=<redacted len=3>}".into()),
            ("id", "a\nb".into(), "{id=<redacted len=3>}".into()),
            // The id rule is `id` or a `_id` suffix - never a substring of the key.
            ("idx", "abc".into(), "{idx=<redacted len=3>}".into()),
            (
                "identity",
                "abc".into(),
                "{identity=<redacted len=3>}".into(),
            ),
            ("_id", "abc".into(), "{_id=abc}".into()),
            // Why: the length cap counts bytes while the placeholder counts characters, so a
            // 64-character Cyrillic id is 128 bytes and degrades. See QA_REPORT F2.
            (
                "id",
                "а".repeat(MAX_TOKEN_LEN),
                format!("{{id=<redacted len={MAX_TOKEN_LEN}>}}"),
            ),
        ];
        for (key, value, expected) in rows {
            assert_eq!(single(key, json!(value)), expected, "key {key:?}");
        }
    }

    /// Why: `reason` was on the disclosure allowlist until the moderation payloads were audited.
    /// Twitch bans and Kick permanent bans put a moderator's free text about a viewer there, and a
    /// one-word slur is token-shaped - so no value-shape rule can make this key safe to disclose.
    #[test]
    fn a_token_shaped_reason_is_still_redacted() {
        for value in ["queue_paused", "ban-evasion-alt-of-bob", "spam"] {
            assert_eq!(
                single("reason", json!(value)),
                format!("{{reason=<redacted len={}>}}", value.chars().count()),
            );
        }
    }

    /// Why: dropping a key from the roster is silent - it degrades to a length placeholder and the
    /// identity collapse stops firing for that shape, so the roster is spelled out here rather than
    /// read back from HANDLE_KEYS. Each name is a key the platform crates actually emit.
    #[test]
    fn every_handle_key_renders_a_twelve_hex_digest_of_its_value() {
        for key in [
            "author",
            "display_name",
            "handle",
            "login",
            "nick",
            "user_login",
            "user_name",
            "username",
        ] {
            let rendered = single(key, json!("bob"));
            let digest = digest_in(&rendered);
            assert_eq!(rendered, format!("{{{key}=<redacted digest={digest}>}}"));
            assert_eq!(digest.len(), DIGEST_BYTES * 2, "key {key:?}");
            assert!(
                digest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                "key {key:?} digest {digest:?}"
            );
            assert!(!rendered.contains("bob"), "key {key:?}");
        }
    }

    /// Why: the roster is matched whole, so raw platform key shapes never reach the digest branch.
    /// They still redact by length, but they do not correlate and they do not collapse siblings.
    #[test]
    fn keys_near_the_handle_roster_are_not_digested() {
        for key in ["user", "name", "chatter_user_login", "Login", "displayName"] {
            assert_eq!(
                single(key, json!("bob")),
                format!("{{{key}=<redacted len=3>}}"),
                "key {key:?}"
            );
        }
    }

    #[test]
    fn strings_outside_the_allowlist_degrade_to_a_character_count() {
        for (payload, expected) in [
            (json!({ "message": "hi" }), "{message=<redacted len=2>}"),
            (json!({ "message": "Привіт" }), "{message=<redacted len=6>}"),
            (json!({ "message": "" }), "{message=<redacted len=0>}"),
            // An array element carries no key, so even an id list loses its disclosure.
            (json!({ "ids": ["abc-123"] }), "{ids=[<redacted len=7>]}"),
        ] {
            assert_eq!(project(payload), expected);
        }
    }

    #[test]
    fn nested_identity_object_collapses_to_its_non_null_ids() {
        assert_eq!(
            project(json!({
                "user": { "id": "u1", "login": "bob", "display_name": "Bob", "message": "hi" }
            })),
            "{user={id=u1}}"
        );
        let two_ids = project(json!({ "user": { "id": "u1", "user_id": "u2", "login": "bob" } }));
        for entry in ["id=u1", "user_id=u2"] {
            assert!(two_ids.contains(entry), "{entry} missing from {two_ids}");
        }
        assert!(!two_ids.contains("login"), "{two_ids}");
    }

    #[test]
    fn identity_object_without_usable_ids_falls_back_to_handle_digests() {
        let null_id = project(json!({ "user": { "id": Value::Null, "login": "bob" } }));
        assert_eq!(
            null_id,
            format!(
                "{{user={{login=<redacted digest={}>}}}}",
                digest_in(&null_id)
            )
        );

        let no_id = project(json!({ "moderator": { "login": "bob", "role": "mod" } }));
        assert_eq!(
            no_id,
            format!(
                "{{moderator={{login=<redacted digest={}>}}}}",
                digest_in(&no_id)
            )
        );
    }

    #[test]
    fn object_without_a_string_handle_key_keeps_every_field() {
        for (payload, entries) in [
            (
                json!({ "queue": { "queue_id": "q1", "depth": 5 } }),
                ["queue_id=q1", "depth=5"],
            ),
            // A handle key holding a non-string is not an identity, so nothing collapses.
            (
                json!({ "user": { "login": 42, "depth": 5 } }),
                ["login=42", "depth=5"],
            ),
        ] {
            let rendered = project(payload);
            for entry in entries {
                assert!(rendered.contains(entry), "{entry} missing from {rendered}");
            }
        }
    }

    /// Why: the root object is the event envelope, not a person - collapsing it would erase the
    /// whole payload of any event that names a viewer at the top level.
    #[test]
    fn root_payload_keeps_every_field_and_digests_top_level_handles() {
        let rendered = project(json!({ "login": "bob", "message": "hi", "channel": "chan" }));
        for entry in [
            format!("login=<redacted digest={}>", digest_in(&rendered)),
            "message=<redacted len=2>".to_owned(),
            "channel=<redacted len=4>".to_owned(),
        ] {
            assert!(rendered.contains(&entry), "{entry} missing from {rendered}");
        }
    }

    /// Why: a per-call salt would still redact, but would break the one thing digests exist for -
    /// following a single viewer across a reproduction trail.
    #[test]
    fn identity_digest_is_stable_within_a_run_and_separates_handles() {
        assert_eq!(identity_digest("bob"), identity_digest("bob"));
        assert_ne!(identity_digest("bob"), identity_digest("bobb"));

        let mut unsalted = Sha256::new();
        unsalted.update(b"bob");
        let bare: String = unsalted
            .finalize()
            .iter()
            .take(DIGEST_BYTES)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_ne!(identity_digest("bob"), bare);
    }

    #[test]
    fn containers_below_the_depth_cap_render_and_deeper_ones_summarise() {
        for (levels, leaf, expected_leaf) in [
            (MAX_DEPTH - 1, json!({ "x": 1, "y": 2 }), "{x=1, y=2}"),
            (MAX_DEPTH, json!({ "x": 1, "y": 2 }), "{2 fields}"),
            (MAX_DEPTH - 1, json!([1, 2, 3]), "[1, 2, 3]"),
            (MAX_DEPTH, json!([1, 2, 3]), "[3 items]"),
        ] {
            let expected = format!(
                "{}{expected_leaf}{}",
                "{a=".repeat(levels),
                "}".repeat(levels)
            );
            assert_eq!(project(nest(levels, leaf)), expected, "levels {levels}");
        }
    }

    #[test]
    fn entries_past_the_cap_are_elided_with_a_count() {
        for extra in [0, 1] {
            let count = MAX_ENTRIES + extra;
            let object: Map<String, Value> =
                (0..count).map(|n| (format!("k{n:02}"), json!(n))).collect();
            let array: Vec<Value> = (0..count).map(|n| json!(n)).collect();
            let suffix = if extra == 0 {
                String::new()
            } else {
                format!(", +{extra} more")
            };

            let rendered_object = project(json!({ "fields": object }));
            assert!(
                rendered_object.ends_with(&format!("k11=11{suffix}}}}}")),
                "object with {count} entries rendered {rendered_object}"
            );
            assert!(!rendered_object.contains("k12"), "{rendered_object}");

            let rendered_array = project(json!({ "items": array }));
            assert!(
                rendered_array.ends_with(&format!("11{suffix}]}}")),
                "array with {count} items rendered {rendered_array}"
            );
        }
    }

    /// Why: this is the whole point of the bridge - a chat event is the densest viewer-authored
    /// payload on the bus, and none of it may reach a log bundle.
    #[test]
    fn no_viewer_authored_text_survives_a_chat_payload_projection() {
        const SENTINELS: &[&str] = &[
            "SENTINEL-CHANNEL",
            "SENTINEL-LOGIN",
            "SENTINEL-DISPLAY",
            "SENTINEL-ROLE",
            "SENTINEL-BODY",
            "SENTINEL-BADGE",
            "SENTINEL-COLOR",
        ];
        let rendered = project(json!({
            "channel": "SENTINEL-CHANNEL",
            "user": {
                "id": "u-1",
                "login": "SENTINEL-LOGIN",
                "display_name": "SENTINEL-DISPLAY",
                "roles": ["SENTINEL-ROLE"],
            },
            "message": "SENTINEL-BODY",
            "badges": ["SENTINEL-BADGE"],
            "color": "SENTINEL-COLOR",
        }));
        for sentinel in SENTINELS {
            assert!(!rendered.contains(sentinel), "{sentinel} in {rendered}");
        }
        // Guards the assertions above against passing on an empty rendering.
        assert!(rendered.contains("user={id=u-1}"), "{rendered}");
    }
}
