use serde::{Deserialize, Serialize};

use forge_events::EventSource;
use forge_types::EventId;

/// Wildcard-capable event filter sent by clients in subscribe/unsubscribe payloads.
/// `"*"` or absent field means wildcard for that axis.
#[derive(Debug, Deserialize, Serialize)]
pub struct WireEventFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// All methods clients may invoke over the WebSocket connection.
///
/// Envelope uses internally tagged serde: the `"request"` JSON field identifies
/// the variant. Field names follow camelCase to match JS overlay conventions.
#[derive(Debug, Deserialize)]
#[serde(tag = "request", rename_all = "camelCase")]
pub enum WsRequest {
    Auth {
        token: String,
    },
    Subscribe {
        events: Vec<WireEventFilter>,
    },
    Unsubscribe {
        events: Vec<WireEventFilter>,
    },
    GetInfo,
    GetActions,
    DoAction {
        #[serde(rename = "actionId")]
        action_id: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    GetCommands,
    GetGlobals,
    GetGlobal {
        name: String,
    },
    SetGlobal {
        name: String,
        value: serde_json::Value,
        #[serde(default)]
        persisted: bool,
    },
    GetUserGlobals {
        #[serde(rename = "broadcasterId")]
        broadcaster_id: String,
        #[serde(rename = "userId", default)]
        user_id: Option<String>,
    },
    TriggerCodeEvent {
        name: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    GetEvents {
        #[serde(default)]
        limit: Option<u32>,
        #[serde(default)]
        since: Option<String>,
    },
    ReplayEvent {
        #[serde(rename = "eventId")]
        event_id: String,
    },
    GetActiveViewers,
    GetOverlayFiles {
        #[serde(default)]
        recursive: bool,
    },
}

/// Server-to-client response; `Ok` data fields are merged into the outer JSON object.
#[derive(Debug)]
pub enum WsResponse {
    Ok(serde_json::Value),
    Error {
        code: Option<String>,
        message: String,
    },
}

/// Outer correlation envelope. Wraps both requests (id + flattened WsRequest)
/// and responses (id + WsResponse).
#[derive(Debug)]
pub struct WsEnvelope<T> {
    pub id: Option<String>,
    pub inner: T,
}

impl<'de> Deserialize<'de> for WsEnvelope<WsRequest> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let mut map: serde_json::Map<String, serde_json::Value> =
            Deserialize::deserialize(deserializer)?;
        let id = match map.remove("id") {
            Some(serde_json::Value::String(s)) => Some(s),
            _ => None,
        };
        let inner =
            WsRequest::deserialize(serde_json::Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self { id, inner })
    }
}

/// The `Ok` variant's data fields are merged into the top-level object alongside
/// `"id"` and `"status"`. The `Error` variant nests under an `"error"` key.
pub fn serialize_response_frame(envelope: &WsEnvelope<WsResponse>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    match &envelope.id {
        Some(id) => {
            map.insert("id".into(), serde_json::Value::String(id.clone()));
        }
        None => {
            map.insert("id".into(), serde_json::Value::Null);
        }
    }
    match &envelope.inner {
        WsResponse::Ok(data) => {
            map.insert("status".into(), "ok".into());
            if let serde_json::Value::Object(fields) = data {
                for (k, v) in fields {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
        WsResponse::Error { code, message } => {
            map.insert("status".into(), "error".into());
            let mut err = serde_json::Map::new();
            if let Some(c) = code {
                err.insert("code".into(), c.clone().into());
            }
            err.insert("message".into(), message.clone().into());
            map.insert("error".into(), serde_json::Value::Object(err));
        }
    }
    serde_json::Value::Object(map)
}

/// Push event frame sent to subscribed clients (§2.3 of RFC-032).
///
/// `time_stamp` serializes as `"timeStamp"` (RFC 3339 UTC with ms precision).
/// `caused_by` is omitted from JSON when absent.
/// `replay` is `false` by default.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPushFrame {
    pub time_stamp: String,
    pub event: EventMetadata,
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EventMetadata {
    pub source: EventSource,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: EventId,
    #[serde(rename = "causedBy", skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<EventId>,
    #[serde(default)]
    pub replay: bool,
}
