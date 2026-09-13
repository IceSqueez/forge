use serde_json::{Value, json};

use super::ids;
use super::ledger::{RecordedSession, RecordedSubscription};

fn metadata(message_type: &str) -> Value {
    json!({
        "message_id": ids::uuid_like(),
        "message_type": message_type,
        "message_timestamp": ids::timestamp_now(),
    })
}

pub(crate) fn welcome(session: &RecordedSession, keepalive_timeout_seconds: u64) -> String {
    json!({
        "metadata": metadata("session_welcome"),
        "payload": {
            "session": {
                "id": session.id,
                "status": "connected",
                "connected_at": session.connected_at,
                "keepalive_timeout_seconds": keepalive_timeout_seconds,
                "reconnect_url": null,
            }
        }
    })
    .to_string()
}

pub(crate) fn keepalive() -> String {
    json!({ "metadata": metadata("session_keepalive"), "payload": {} }).to_string()
}

pub(crate) fn notification(subscription: &RecordedSubscription, event: &Value) -> String {
    let mut metadata = metadata("notification");
    metadata["subscription_type"] = json!(subscription.subscription_type);
    metadata["subscription_version"] = json!(subscription.version);
    json!({
        "metadata": metadata,
        "payload": {
            "subscription": {
                "id": subscription.id,
                "status": "enabled",
                "type": subscription.subscription_type,
                "version": subscription.version,
                "cost": 0,
                "condition": subscription.condition,
                "transport": { "method": "websocket", "session_id": subscription.session_id },
                "created_at": subscription.created_at,
            },
            "event": event,
        }
    })
    .to_string()
}

pub(crate) fn reconnect(session: &RecordedSession, reconnect_url: &str) -> String {
    json!({
        "metadata": metadata("session_reconnect"),
        "payload": {
            "session": {
                "id": session.id,
                "status": "reconnecting",
                "connected_at": session.connected_at,
                "keepalive_timeout_seconds": null,
                "reconnect_url": reconnect_url,
            }
        }
    })
    .to_string()
}
