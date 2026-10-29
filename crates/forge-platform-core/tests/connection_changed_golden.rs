#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;

use forge_platform_core::{ConnectionState, ConnectionTransition, connection_state_changed_event};
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("fixtures/observability/platform.connection.changed.json");

fn with_keys_sorted(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let sorted: BTreeMap<String, Value> = fields
                .into_iter()
                .map(|(key, value)| (key, with_keys_sorted(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(with_keys_sorted).collect()),
        other => other,
    }
}

#[test]
fn connection_changed_payload_matches_the_golden_for_each_state_and_transition() {
    let emitted: Vec<Value> = [
        (
            "twitch",
            ConnectionState::Connecting,
            ConnectionTransition::Expected,
        ),
        (
            "youtube",
            ConnectionState::Connected,
            ConnectionTransition::Expected,
        ),
        (
            "kick",
            ConnectionState::Reconnecting,
            ConnectionTransition::Unexpected,
        ),
        (
            "obs",
            ConnectionState::Disconnected,
            ConnectionTransition::Unexpected,
        ),
        (
            "vtube",
            ConnectionState::Disconnected,
            ConnectionTransition::Expected,
        ),
    ]
    .into_iter()
    .map(|(platform, state, transition)| {
        let event = connection_state_changed_event(platform, state, transition);
        json!({ "kind": event.kind, "source": event.source, "payload": event.payload })
    })
    .collect();

    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&with_keys_sorted(Value::Array(emitted))).unwrap()
    );

    assert_eq!(rendered, GOLDEN);
}
