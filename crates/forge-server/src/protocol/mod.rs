use std::sync::{Arc, atomic::Ordering};

use serde::{Deserialize, Serialize};

use forge_events::EventSource;
use forge_runtime::EventBus;
use forge_storage::DataProvider;
use forge_types::EventId;

use crate::auth::AuthState;
use crate::bus_adapter::BusAdapter;
use crate::ws_client::WsClient;

/// Wildcard-capable event filter sent by clients in subscribe/unsubscribe payloads.
/// `"*"` or absent field means wildcard for that axis.
#[derive(Debug, Deserialize)]
pub struct WireEventFilter {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(rename = "type", default)]
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

/// Serializes a response envelope to a JSON value.
///
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

/// Metadata nested inside an `EventPushFrame`.
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

/// Per-connection context passed to every dispatch call.
pub struct DispatchContext {
    pub bus: Arc<EventBus>,
    pub bus_adapter: Arc<BusAdapter>,
    pub dp: Arc<dyn DataProvider>,
    pub auth_state: Arc<AuthState>,
    pub client: Arc<WsClient>,
    pub auth_required_for_reads: bool,
}

fn not_implemented() -> WsResponse {
    WsResponse::Error {
        code: None,
        message: "method not implemented".to_owned(),
    }
}

fn unauthenticated() -> WsResponse {
    WsResponse::Error {
        code: Some("UNAUTHENTICATED".to_owned()),
        message: "authentication required".to_owned(),
    }
}

fn is_authenticated(ctx: &DispatchContext) -> bool {
    ctx.client.authenticated.load(Ordering::Acquire)
}

pub async fn dispatch(req: WsEnvelope<WsRequest>, ctx: &DispatchContext) -> WsEnvelope<WsResponse> {
    let id = req.id.clone();
    let inner = route(req.inner, ctx).await;
    WsEnvelope { id, inner }
}

async fn route(req: WsRequest, ctx: &DispatchContext) -> WsResponse {
    match req {
        WsRequest::Auth { .. } => not_implemented(),

        WsRequest::Subscribe { .. } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::Unsubscribe { .. } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetInfo => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetActions => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::DoAction { .. } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetCommands => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetGlobals => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetGlobal { .. } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::SetGlobal { .. } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetUserGlobals { .. } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::TriggerCodeEvent { .. } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetEvents { .. } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::ReplayEvent { .. } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetActiveViewers => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }

        WsRequest::GetOverlayFiles { .. } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            not_implemented()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use forge_runtime::{EventBus, NullEventLogRepo};
    use forge_storage::DataProvider;

    use super::*;
    use crate::bus_adapter::{BusAdapter, ClientId};
    use crate::test_dp::null_dp;
    use crate::ws_client::WsClient;

    fn make_ctx(authenticated: bool, auth_required_for_reads: bool) -> DispatchContext {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        let dp: Arc<dyn DataProvider> = null_dp();
        let auth_state = AuthState::for_test(auth_required_for_reads, "test-token");
        let drop_counter = Arc::new(AtomicU64::new(0));
        let client = Arc::new(WsClient::new(
            ClientId::next(),
            "127.0.0.1:0".parse().unwrap(),
            Arc::clone(&drop_counter),
        ));
        client.authenticated.store(authenticated, Ordering::Relaxed);
        DispatchContext {
            bus,
            bus_adapter,
            dp,
            auth_state,
            client,
            auth_required_for_reads,
        }
    }

    #[test]
    fn parse_request_envelope_get_info() {
        let json = r#"{"id":"1","request":"getInfo"}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, Some("1".to_owned()));
        assert!(matches!(env.inner, WsRequest::GetInfo));
    }

    #[test]
    fn parse_request_envelope_do_action_camel_case_fields() {
        let json = r#"{"id":"5","request":"doAction","actionId":"abc","args":{"user":"Alice"}}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, Some("5".to_owned()));
        match env.inner {
            WsRequest::DoAction { action_id, args } => {
                assert_eq!(action_id, "abc");
                assert_eq!(args["user"], "Alice");
            }
            other => panic!("expected DoAction, got {other:?}"),
        }
    }

    #[test]
    fn parse_request_envelope_no_id_field() {
        let json = r#"{"request":"getActions"}"#;
        let env: WsEnvelope<WsRequest> = serde_json::from_str(json).unwrap();
        assert_eq!(env.id, None);
        assert!(matches!(env.inner, WsRequest::GetActions));
    }

    #[test]
    fn serialize_response_ok_data_flattened() {
        let env = WsEnvelope {
            id: Some("1".to_owned()),
            inner: WsResponse::Ok(serde_json::json!({"version": "0.1.0-alpha.9"})),
        };
        let json = serialize_response_frame(&env);
        assert_eq!(json["id"], "1");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["version"], "0.1.0-alpha.9");
        assert!(json.get("inner").is_none());
    }

    #[test]
    fn serialize_response_error_shape() {
        let env = WsEnvelope {
            id: Some("2".to_owned()),
            inner: WsResponse::Error {
                code: Some("NOT_FOUND".to_owned()),
                message: "action not found".to_owned(),
            },
        };
        let json = serialize_response_frame(&env);
        assert_eq!(json["id"], "2");
        assert_eq!(json["status"], "error");
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert_eq!(json["error"]["message"], "action not found");
    }

    #[test]
    fn serialize_response_null_id_when_absent() {
        let env = WsEnvelope {
            id: None,
            inner: WsResponse::Ok(serde_json::json!({})),
        };
        let json = serialize_response_frame(&env);
        assert!(json["id"].is_null());
    }

    #[tokio::test]
    async fn unauthenticated_do_action_returns_unauthenticated_error() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("5".to_owned()),
            inner: WsRequest::DoAction {
                action_id: "fake-id".to_owned(),
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("5".to_owned()));
        match resp.inner {
            WsResponse::Error {
                code: Some(code),
                message,
            } => {
                assert_eq!(code, "UNAUTHENTICATED");
                assert!(!message.is_empty());
            }
            other => panic!("expected UNAUTHENTICATED error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unauthenticated_set_global_returns_unauthenticated_error() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("6".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(42),
                persisted: true,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn authenticated_do_action_returns_not_implemented_stub() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("7".to_owned()),
            inner: WsRequest::DoAction {
                action_id: "fake-id".to_owned(),
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("7".to_owned()));
        match resp.inner {
            WsResponse::Error {
                code: None,
                message,
            } => {
                assert!(
                    message.contains("not implemented"),
                    "expected 'not implemented' in message, got: {message}"
                );
            }
            other => panic!("expected not-implemented stub error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_only_allowed_without_auth_by_default() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("8".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: None,
                message,
            } => {
                assert!(message.contains("not implemented"));
            }
            other => panic!("expected not-implemented stub, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_only_blocked_when_auth_required_for_reads() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("9".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "UNAUTHENTICATED"),
            other => panic!("expected UNAUTHENTICATED, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn authenticated_replay_event_returns_not_implemented() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::ReplayEvent {
                event_id: "01JVKR7X8QD0GEEMHC4Z3F2P1K".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert!(matches!(resp.inner, WsResponse::Error { code: None, .. }));
    }
}
