use std::collections::HashSet;
use std::sync::{Arc, atomic::Ordering};

use serde::{Deserialize, Serialize};

use forge_events::{Event, EventSource};
use forge_runtime::{ActionEngineHandle, BusError, EventBus, ExecutionRequest};
use forge_storage::{CredentialsRepo, DataProvider, GlobalsRepo, UserGlobalsRepo};
use forge_types::{ActionId, ArgStack, CommandPermission, EventId, Variant};

use crate::auth::AuthState;
use crate::bus_adapter::{BusAdapter, ClientFilterSet, EventFilter};
use crate::server_info::ServerInfo;
use crate::ws_client::WsClient;

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
    pub credentials: Arc<dyn CredentialsRepo>,
    pub server_info: Arc<ServerInfo>,
    pub action_engine: Arc<ActionEngineHandle>,
}

const PLATFORM_PREFIXES: &[&str] = &["twitch:", "youtube:", "kick:", "trovo:"];

async fn build_connected_accounts(creds: &dyn CredentialsRepo) -> Vec<serde_json::Value> {
    let ids = match creds.list_ids().await {
        Ok(ids) => ids,
        Err(_) => return vec![],
    };
    ids.into_iter()
        .filter_map(|id| {
            let s = id.as_str();
            for prefix in PLATFORM_PREFIXES {
                if let Some(login) = s.strip_prefix(prefix) {
                    let platform = &prefix[..prefix.len() - 1];
                    return Some(serde_json::json!({
                        "platform": platform,
                        "login": login,
                    }));
                }
            }
            None
        })
        .collect()
}

async fn build_connected_clients(
    server_info: &ServerInfo,
    bus_adapter: &BusAdapter,
) -> Vec<serde_json::Value> {
    let clients = server_info.connected_clients.read().await;
    let mut result = Vec::with_capacity(clients.len());
    for (id, client) in clients.iter() {
        let subs = bus_adapter.current_subscriptions(*id).await;
        let subscriptions: Vec<serde_json::Value> = subs
            .iter()
            .map(|f| {
                let source_str = match f.source {
                    None => "*".to_owned(),
                    Some(s) => serde_json::to_value(s)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "*".to_owned()),
                };
                let kind_str = f.kind.as_deref().unwrap_or("*");
                serde_json::json!({ "source": source_str, "type": kind_str })
            })
            .collect();

        let identification = client.identification.load_full();
        let client_type = client.client_type.load_full();
        let uptime = client.uptime();

        result.push(serde_json::json!({
            "client_id": id.to_string(),
            "identification": identification.as_str(),
            "remote_addr": client.remote_addr.ip().to_string(),
            "client_type": client_type.type_str(),
            "subscriptions": subscriptions,
            "events_per_second": client.events_per_second(),
            "uptime_seconds": uptime.whole_seconds(),
            "bytes_sent": client.bytes_sent_session.load(Ordering::Relaxed),
        }));
    }
    result
}

async fn handle_get_info(ctx: &DispatchContext) -> WsResponse {
    let connected_accounts = build_connected_accounts(ctx.credentials.as_ref()).await;
    let connected_clients = build_connected_clients(&ctx.server_info, &ctx.bus_adapter).await;
    let bw = &ctx.server_info.bandwidth;
    WsResponse::Ok(serde_json::json!({
        "version": ctx.server_info.version,
        "uptime_seconds": ctx.server_info.uptime_seconds(),
        "connected_accounts": connected_accounts,
        "available_platforms": ["twitch"],
        "connected_clients": connected_clients,
        "bandwidth": {
            "outbound_bytes_per_second": bw.current_bps(),
            "outbound_bytes_total": bw.total(),
            "peak_outbound_bytes_per_second": bw.peak(),
        },
    }))
}

fn parse_wire_filter(wf: &WireEventFilter) -> EventFilter {
    let source = match wf.source.as_deref() {
        None | Some("*") => None,
        Some(s) => serde_json::from_value(serde_json::Value::String(s.to_owned())).ok(),
    };
    let kind = match wf.kind.as_deref() {
        None | Some("*") => None,
        Some(k) => Some(k.to_owned()),
    };
    EventFilter::new(source, kind)
}

fn not_implemented() -> WsResponse {
    WsResponse::Error {
        code: None,
        message: "method not implemented".to_owned(),
    }
}

fn variant_to_wire_value(v: &Variant) -> serde_json::Value {
    match v {
        Variant::Int(n) => serde_json::json!(n),
        Variant::Float(f) => serde_json::json!(f),
        Variant::Bool(b) => serde_json::json!(b),
        Variant::String(s) => serde_json::Value::String(s.clone()),
        Variant::Datetime(dt) => serde_json::Value::String(
            dt.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ),
        Variant::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(variant_to_wire_value).collect())
        }
        Variant::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), variant_to_wire_value(val)))
                .collect(),
        ),
    }
}

fn permission_str(p: &CommandPermission) -> &'static str {
    match p {
        CommandPermission::Everyone => "everyone",
        CommandPermission::Subscriber => "subscriber",
        CommandPermission::Vip => "vip",
        CommandPermission::Moderator => "moderator",
        CommandPermission::Broadcaster => "broadcaster",
    }
}

async fn handle_get_commands(ctx: &DispatchContext) -> WsResponse {
    let commands = match ctx.dp.command_repo().list().await {
        Ok(list) => list,
        Err(e) => {
            return WsResponse::Error {
                code: Some("RUNTIME_ERROR".to_owned()),
                message: e.to_string(),
            };
        }
    };
    let wire_commands: Vec<serde_json::Value> = commands
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id.to_string(),
                "command": c.name,
                "action_id": c.action_id.to_string(),
                "cooldown_seconds": c.cooldown_secs,
                "enabled": true,
                "permission_level": permission_str(&c.permission),
            })
        })
        .collect();
    WsResponse::Ok(serde_json::json!({ "commands": wire_commands }))
}

async fn handle_get_globals(ctx: &DispatchContext) -> WsResponse {
    let globals_repo: &dyn GlobalsRepo = ctx.dp.as_ref();
    let globals = match globals_repo.list().await {
        Ok(list) => list,
        Err(e) => {
            return WsResponse::Error {
                code: Some("RUNTIME_ERROR".to_owned()),
                message: e.to_string(),
            };
        }
    };
    let wire_globals: Vec<serde_json::Value> = globals
        .iter()
        .map(|g| {
            let last_modified = g
                .last_modified
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default();
            serde_json::json!({
                "key": g.name,
                "value": variant_to_wire_value(&g.value),
                "type": g.value.type_tag().to_string(),
                "persisted": g.persisted,
                "reads": g.reads,
                "writes": g.writes,
                "last_modified": last_modified,
            })
        })
        .collect();
    WsResponse::Ok(serde_json::json!({ "globals": wire_globals }))
}

async fn handle_get_global(name: String, ctx: &DispatchContext) -> WsResponse {
    let globals_repo: &dyn GlobalsRepo = ctx.dp.as_ref();
    match globals_repo.get(&name).await {
        Ok(Some(value)) => WsResponse::Ok(serde_json::json!({
            "name": name,
            "value": variant_to_wire_value(&value),
            "type": value.type_tag().to_string(),
        })),
        Ok(None) => WsResponse::Error {
            code: Some("NOT_FOUND".to_owned()),
            message: format!("global '{name}' not found"),
        },
        Err(e) => WsResponse::Error {
            code: Some("RUNTIME_ERROR".to_owned()),
            message: e.to_string(),
        },
    }
}

async fn handle_set_global(
    name: String,
    value: serde_json::Value,
    persisted: bool,
    ctx: &DispatchContext,
) -> WsResponse {
    let variant = match Variant::from_json(value) {
        Ok(v) => v,
        Err(_) => {
            return WsResponse::Error {
                code: Some("INVALID_PAYLOAD".to_owned()),
                message: "value must be a non-null JSON scalar, array, or object".to_owned(),
            };
        }
    };
    let event_payload = serde_json::json!({
        "name": name,
        "value": variant_to_wire_value(&variant),
        "persisted": persisted,
        "via": "ws_api",
    });
    let globals_repo: &dyn GlobalsRepo = ctx.dp.as_ref();
    match globals_repo.set(&name, variant, persisted).await {
        Ok(()) => {
            ctx.bus
                .publish(Event::new(EventSource::Server, "global.set", event_payload));
            WsResponse::Ok(serde_json::json!({ "ok": true }))
        }
        Err(e) => WsResponse::Error {
            code: Some("RUNTIME_ERROR".to_owned()),
            message: e.to_string(),
        },
    }
}

async fn handle_get_user_globals(
    broadcaster_id: String,
    user_id: Option<String>,
    ctx: &DispatchContext,
) -> WsResponse {
    let user_globals_repo: &dyn UserGlobalsRepo = ctx.dp.as_ref();
    let result = match user_id {
        Some(ref uid) => user_globals_repo.list_for_user(&broadcaster_id, uid).await,
        None => {
            user_globals_repo
                .list_for_broadcaster(&broadcaster_id)
                .await
        }
    };
    match result {
        Ok(list) => {
            let wire: Vec<serde_json::Value> = list
                .iter()
                .map(|e| {
                    let last_modified = e
                        .last_modified
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_default();
                    serde_json::json!({
                        "user_id": e.user_id,
                        "key": e.name,
                        "value": variant_to_wire_value(&e.value),
                        "type": e.value.type_tag().to_string(),
                        "last_modified": last_modified,
                    })
                })
                .collect();
            WsResponse::Ok(serde_json::json!({ "globals": wire }))
        }
        Err(e) => WsResponse::Error {
            code: Some("RUNTIME_ERROR".to_owned()),
            message: e.to_string(),
        },
    }
}

fn valid_code_event_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

async fn handle_trigger_code_event(
    name: String,
    args: serde_json::Value,
    ctx: &DispatchContext,
) -> WsResponse {
    if !valid_code_event_name(&name) {
        return WsResponse::Error {
            code: Some("INVALID_PAYLOAD".to_owned()),
            message: "name must match [a-z0-9_]{1,64}".to_owned(),
        };
    }
    let kind = format!("custom.{name}");
    ctx.bus
        .publish(Event::new(EventSource::Server, &kind, args));
    WsResponse::Ok(serde_json::json!({ "ok": true }))
}

async fn handle_get_actions(ctx: &DispatchContext) -> WsResponse {
    let actions = match ctx.dp.action_repo().list().await {
        Ok(list) => list,
        Err(e) => {
            return WsResponse::Error {
                code: Some("RUNTIME_ERROR".to_owned()),
                message: e.to_string(),
            };
        }
    };
    let wire_actions: Vec<serde_json::Value> = actions
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id.to_string(),
                "name": a.name,
                "group": a.group,
                "queue_id": a.queue_id.to_string(),
                "enabled": a.enabled,
                "concurrent": a.concurrent,
                "bypass_queue_pause": a.bypass_pause,
                "description": a.description,
                "sub_action_count": a.sub_actions.len(),
            })
        })
        .collect();
    WsResponse::Ok(serde_json::json!({ "actions": wire_actions }))
}

async fn handle_do_action(
    action_id: String,
    args: serde_json::Value,
    ctx: &DispatchContext,
) -> WsResponse {
    let aid: ActionId = match serde_json::from_value(serde_json::Value::String(action_id.clone())) {
        Ok(id) => id,
        Err(_) => {
            return WsResponse::Error {
                code: Some("INVALID_PAYLOAD".to_owned()),
                message: "invalid action id format".to_owned(),
            };
        }
    };

    match ctx.dp.action_repo().get(aid).await {
        Ok(None) => {
            return WsResponse::Error {
                code: Some("NOT_FOUND".to_owned()),
                message: "action not found".to_owned(),
            };
        }
        Ok(Some(_)) => {}
        Err(e) => {
            return WsResponse::Error {
                code: Some("RUNTIME_ERROR".to_owned()),
                message: e.to_string(),
            };
        }
    }

    let trigger_event_id = EventId::new();

    ctx.bus.publish(Event::new(
        EventSource::Server,
        "action.invoked",
        serde_json::json!({
            "action_id": action_id,
            "user_via": "ws_api",
        }),
    ));

    let initial_args = build_arg_stack(args);

    match ctx
        .action_engine
        .dispatch(ExecutionRequest {
            action_id: aid,
            trigger_event_id,
            initial_args,
        })
        .await
    {
        Ok(()) => WsResponse::Ok(serde_json::json!({
            "ok": true,
            "execution_id": trigger_event_id.to_string(),
        })),
        Err(e) => WsResponse::Error {
            code: Some("RUNTIME_ERROR".to_owned()),
            message: e.to_string(),
        },
    }
}

fn build_arg_stack(args: serde_json::Value) -> ArgStack {
    let obj = match args {
        serde_json::Value::Object(m) => m,
        _ => return ArgStack::new(),
    };
    obj.into_iter()
        .filter_map(|(k, v)| Variant::from_json(v).ok().map(|vv| (k, vv)))
        .fold(ArgStack::new(), |stack, (k, v)| stack.set(k, v))
}

async fn handle_get_events(
    limit: Option<u32>,
    since: Option<String>,
    ctx: &DispatchContext,
) -> WsResponse {
    let limit = limit.unwrap_or(100).min(500) as usize;
    let since_id: Option<EventId> = match since {
        None => None,
        Some(s) => match serde_json::from_value::<EventId>(serde_json::Value::String(s)) {
            Ok(id) => Some(id),
            Err(_) => {
                return WsResponse::Error {
                    code: Some("INVALID_PAYLOAD".to_owned()),
                    message: "since must be a valid event id".to_owned(),
                };
            }
        },
    };
    let events = ctx.bus.recent_since(limit, since_id).await;
    let wire: Vec<serde_json::Value> = events
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    WsResponse::Ok(serde_json::json!({ "events": wire }))
}

async fn handle_replay_event(event_id: String, ctx: &DispatchContext) -> WsResponse {
    let eid: EventId = match serde_json::from_value(serde_json::Value::String(event_id)) {
        Ok(id) => id,
        Err(_) => {
            return WsResponse::Error {
                code: Some("NOT_FOUND".to_owned()),
                message: "event not found".to_owned(),
            };
        }
    };
    match ctx.bus.replay_and_publish(eid).await {
        Ok(()) => WsResponse::Ok(serde_json::json!({ "ok": true })),
        Err(BusError::EventNotFound(_)) => WsResponse::Error {
            code: Some("NOT_FOUND".to_owned()),
            message: "event not found".to_owned(),
        },
        Err(BusError::Storage(e)) => WsResponse::Error {
            code: Some("RUNTIME_ERROR".to_owned()),
            message: e.to_string(),
        },
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

async fn handle_authenticate(token: String, ctx: &DispatchContext) -> WsResponse {
    if ctx.auth_state.verify(&token).await {
        ctx.client.authenticated.store(true, Ordering::SeqCst);
        WsResponse::Ok(serde_json::json!({ "authenticated": true }))
    } else {
        WsResponse::Error {
            code: Some("AUTH_FAILED".to_owned()),
            message: "invalid token".to_owned(),
        }
    }
}

pub async fn dispatch(req: WsEnvelope<WsRequest>, ctx: &DispatchContext) -> WsEnvelope<WsResponse> {
    let id = req.id.clone();
    let inner = route(req.inner, ctx).await;
    WsEnvelope { id, inner }
}

async fn route(req: WsRequest, ctx: &DispatchContext) -> WsResponse {
    match req {
        WsRequest::Auth { token } => handle_authenticate(token, ctx).await,

        WsRequest::Subscribe { events } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            let new_filters: HashSet<EventFilter> = events.iter().map(parse_wire_filter).collect();
            let mut current = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
            current.extend(new_filters);
            ctx.bus_adapter
                .update_subscriptions(ctx.client.id, ClientFilterSet::new(current))
                .await;
            WsResponse::Ok(serde_json::json!({ "subscribed": events }))
        }

        WsRequest::Unsubscribe { events } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            let to_remove: HashSet<EventFilter> = events.iter().map(parse_wire_filter).collect();
            let mut current = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
            current.retain(|f| !to_remove.contains(f));
            ctx.bus_adapter
                .update_subscriptions(ctx.client.id, ClientFilterSet::new(current))
                .await;
            WsResponse::Ok(serde_json::json!({ "unsubscribed": events }))
        }

        WsRequest::GetInfo => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_info(ctx).await
        }

        WsRequest::GetActions => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_actions(ctx).await
        }

        WsRequest::DoAction { action_id, args } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_do_action(action_id, args, ctx).await
        }

        WsRequest::GetCommands => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_commands(ctx).await
        }

        WsRequest::GetGlobals => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_globals(ctx).await
        }

        WsRequest::GetGlobal { name } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_global(name, ctx).await
        }

        WsRequest::SetGlobal {
            name,
            value,
            persisted,
        } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_set_global(name, value, persisted, ctx).await
        }

        WsRequest::GetUserGlobals {
            broadcaster_id,
            user_id,
        } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_user_globals(broadcaster_id, user_id, ctx).await
        }

        WsRequest::TriggerCodeEvent { name, args } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_trigger_code_event(name, args, ctx).await
        }

        WsRequest::GetEvents { limit, since } => {
            if ctx.auth_required_for_reads && !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_get_events(limit, since, ctx).await
        }

        WsRequest::ReplayEvent { event_id } => {
            if !is_authenticated(ctx) {
                return unauthenticated();
            }
            handle_replay_event(event_id, ctx).await
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
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use forge_events::EventSource;
    use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry, spawn_action_engine};
    use forge_storage::DataProvider;
    use forge_types::{Action, ActionId, LogLevel, QueueId, SubActionSpec};

    use super::*;
    use crate::bus_adapter::{BusAdapter, ClientFilterSet, ClientId, EventFilter};
    use crate::server_info::ServerInfo;
    use forge_storage::GlobalEntry;
    use time::OffsetDateTime;

    use crate::test_dp::{VecActionDp, VecCommandDp, VecGlobalsDp, null_creds, null_dp};
    use crate::ws_client::WsClient;

    fn make_engine(bus: &Arc<EventBus>, dp: &Arc<dyn DataProvider>) -> Arc<ActionEngineHandle> {
        let registry = Arc::new(ScriptRegistry::new());
        Arc::new(spawn_action_engine(
            Arc::clone(bus),
            Arc::clone(dp),
            registry,
            None,
        ))
    }

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
        let action_engine = make_engine(&bus, &dp);
        DispatchContext {
            bus,
            bus_adapter,
            dp,
            auth_state,
            client,
            auth_required_for_reads,
            credentials: null_creds(),
            server_info: ServerInfo::new(),
            action_engine,
        }
    }

    fn make_ctx_with_dp(authenticated: bool, dp: Arc<dyn DataProvider>) -> DispatchContext {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        let auth_state = AuthState::for_test(false, "test-token");
        let drop_counter = Arc::new(AtomicU64::new(0));
        let client = Arc::new(WsClient::new(
            ClientId::next(),
            "127.0.0.1:0".parse().unwrap(),
            Arc::clone(&drop_counter),
        ));
        client.authenticated.store(authenticated, Ordering::Relaxed);
        let action_engine = make_engine(&bus, &dp);
        DispatchContext {
            bus,
            bus_adapter,
            dp,
            auth_state,
            client,
            auth_required_for_reads: false,
            credentials: null_creds(),
            server_info: ServerInfo::new(),
            action_engine,
        }
    }

    async fn make_registered_ctx(
        authenticated: bool,
        auth_required_for_reads: bool,
    ) -> DispatchContext {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let bus_adapter = BusAdapter::new(Arc::clone(&bus));
        let dp: Arc<dyn DataProvider> = null_dp();
        let auth_state = AuthState::for_test(auth_required_for_reads, "test-token");
        let (handle, _rx) = bus_adapter
            .register_client(ClientFilterSet::new(HashSet::new()))
            .await;
        let client = Arc::new(WsClient::new(
            handle.id,
            "127.0.0.1:0".parse().unwrap(),
            Arc::clone(&handle.drop_counter),
        ));
        client.authenticated.store(authenticated, Ordering::Relaxed);
        let action_engine = make_engine(&bus, &dp);
        DispatchContext {
            bus,
            bus_adapter,
            dp,
            auth_state,
            client,
            auth_required_for_reads,
            credentials: null_creds(),
            server_info: ServerInfo::new(),
            action_engine,
        }
    }

    fn sample_action() -> Action {
        Action {
            id: ActionId::new(),
            name: "Test Action".to_string(),
            group: Some("Chat".to_string()),
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            description: Some("A test action".to_string()),
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "hello".to_string(),
            }],
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
    async fn get_actions_returns_empty_list_for_null_dp() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("5".to_owned()),
            inner: WsRequest::GetActions,
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("5".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["actions"].is_array());
        assert_eq!(json["actions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_actions_returns_wire_shape_for_seeded_action() {
        let action = sample_action();
        let action_id_str = action.id.to_string();
        let dp = VecActionDp::with_actions(vec![action]);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("5".to_owned()),
            inner: WsRequest::GetActions,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let actions = json["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["id"].as_str().unwrap(), action_id_str);
        assert_eq!(actions[0]["name"], "Test Action");
        assert_eq!(actions[0]["group"], "Chat");
        assert_eq!(actions[0]["enabled"], true);
        assert_eq!(actions[0]["concurrent"], false);
        assert_eq!(actions[0]["bypass_queue_pause"], false);
        assert_eq!(actions[0]["sub_action_count"], 1u64);
        assert!(actions[0]["queue_id"].is_string());
    }

    #[tokio::test]
    async fn do_action_valid_id_returns_ok_with_execution_id() {
        let action = sample_action();
        let action_id_str = action.id.to_string();
        let dp = VecActionDp::with_actions(vec![action]);
        let ctx = make_ctx_with_dp(true, dp);
        let req = WsEnvelope {
            id: Some("6".to_owned()),
            inner: WsRequest::DoAction {
                action_id: action_id_str,
                args: serde_json::json!({ "user": "Alice", "count": 3 }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("6".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);
        assert!(json["execution_id"].is_string());
        assert!(!json["execution_id"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn do_action_nonexistent_id_returns_not_found() {
        let ctx = make_ctx(true, false);
        let nonexistent = ActionId::new().to_string();
        let req = WsEnvelope {
            id: Some("7".to_owned()),
            inner: WsRequest::DoAction {
                action_id: nonexistent,
                args: serde_json::Value::Null,
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("7".to_owned()));
        match &resp.inner {
            WsResponse::Error {
                code: Some(code),
                message,
            } => {
                assert_eq!(code, "NOT_FOUND");
                assert!(!message.is_empty());
            }
            other => panic!("expected NOT_FOUND error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_info_succeeds_without_auth_by_default() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("8".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["version"].is_string());
        assert!(json["uptime_seconds"].is_number());
        assert!(json["connected_clients"].is_array());
        assert!(json["bandwidth"].is_object());
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
    async fn get_events_empty_bus_returns_empty_list() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("13".to_owned()),
            inner: WsRequest::GetEvents {
                limit: None,
                since: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["events"].is_array());
        assert_eq!(json["events"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_events_with_three_events_returns_all_in_shape() {
        let ctx = make_ctx(false, false);
        for i in 0u32..3 {
            ctx.bus.publish(Event::new(
                EventSource::Core,
                "test.event",
                serde_json::json!({ "i": i }),
            ));
        }
        let req = WsEnvelope {
            id: Some("13".to_owned()),
            inner: WsRequest::GetEvents {
                limit: None,
                since: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let events = json["events"].as_array().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events[0]["id"].is_string());
        assert!(events[0]["source"].is_string());
        assert!(events[0]["kind"].is_string());
        assert!(events[0]["timestamp"].is_string());
        assert!(events[0]["payload"].is_object());
        assert_eq!(events[0]["replay"], false);
    }

    #[tokio::test]
    async fn get_events_since_with_limit_returns_events_after_anchor() {
        let ctx = make_ctx(false, false);
        let ev1 = Event::new(EventSource::Core, "test.a", serde_json::json!({}));
        let anchor_id = ev1.id.to_string();
        ctx.bus.publish(ev1);
        ctx.bus.publish(Event::new(
            EventSource::Core,
            "test.b",
            serde_json::json!({}),
        ));
        ctx.bus.publish(Event::new(
            EventSource::Core,
            "test.c",
            serde_json::json!({}),
        ));

        let req = WsEnvelope {
            id: Some("13".to_owned()),
            inner: WsRequest::GetEvents {
                limit: Some(1),
                since: Some(anchor_id),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let events = json["events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "test.c");
    }

    #[tokio::test]
    async fn replay_event_without_auth_returns_unauthenticated() {
        let ctx = make_ctx(false, false);
        let ev = Event::new(EventSource::Core, "test.event", serde_json::json!({}));
        let ev_id = ev.id.to_string();
        ctx.bus.publish(ev);
        let req = WsEnvelope {
            id: Some("14".to_owned()),
            inner: WsRequest::ReplayEvent { event_id: ev_id },
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
    async fn replay_event_nonexistent_id_returns_not_found() {
        let ctx = make_ctx(true, false);
        let ghost_id = forge_types::EventId::new().to_string();
        let req = WsEnvelope {
            id: Some("14".to_owned()),
            inner: WsRequest::ReplayEvent { event_id: ghost_id },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "NOT_FOUND"),
            other => panic!("expected NOT_FOUND, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn replay_event_valid_id_returns_ok_and_emits_replayed_event() {
        let ctx = make_ctx(true, false);
        let original = Event::new(EventSource::Core, "test.original", serde_json::json!({}));
        let original_id = original.id.to_string();
        ctx.bus.publish(original);

        let mut bus_sub = ctx.bus.subscribe();

        let req = WsEnvelope {
            id: Some("14".to_owned()),
            inner: WsRequest::ReplayEvent {
                event_id: original_id,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let replayed = bus_sub.recv().await.unwrap();
        assert_eq!(replayed.kind, "test.original");
        assert!(replayed.replay);
    }

    #[tokio::test]
    async fn subscribe_registers_filter_for_client() {
        let ctx = make_registered_ctx(false, false).await;
        let req = WsEnvelope {
            id: Some("2".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("chat.message".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("2".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["subscribed"].is_array());

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert!(subs.contains(&EventFilter::new(
            Some(EventSource::Twitch),
            Some("chat.message".to_owned()),
        )));
    }

    #[tokio::test]
    async fn unsubscribe_removes_filter_from_client() {
        let ctx = make_registered_ctx(false, false).await;
        let filter = EventFilter::new(Some(EventSource::Twitch), Some("chat.message".to_owned()));
        ctx.bus_adapter
            .update_subscriptions(
                ctx.client.id,
                ClientFilterSet::new(HashSet::from([filter.clone()])),
            )
            .await;

        let req = WsEnvelope {
            id: Some("3".to_owned()),
            inner: WsRequest::Unsubscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("chat.message".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        assert_eq!(resp.id, Some("3".to_owned()));
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["unsubscribed"].is_array());

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert!(!subs.contains(&filter));
    }

    #[tokio::test]
    async fn subscribe_is_additive_across_calls() {
        let ctx = make_registered_ctx(false, false).await;
        let req1 = WsEnvelope {
            id: Some("2".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: None,
                }],
            },
        };
        dispatch(req1, &ctx).await;

        let req2 = WsEnvelope {
            id: Some("3".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("obs".to_owned()),
                    kind: None,
                }],
            },
        };
        dispatch(req2, &ctx).await;

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert_eq!(subs.len(), 2);
    }

    #[tokio::test]
    async fn subscribe_wildcard_type_registers_kind_wildcard() {
        let ctx = make_registered_ctx(false, false).await;
        let req = WsEnvelope {
            id: Some("4".to_owned()),
            inner: WsRequest::Subscribe {
                events: vec![WireEventFilter {
                    source: Some("twitch".to_owned()),
                    kind: Some("*".to_owned()),
                }],
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");

        let subs = ctx.bus_adapter.current_subscriptions(ctx.client.id).await;
        assert!(subs.contains(&EventFilter::new(Some(EventSource::Twitch), None)));
    }

    #[tokio::test]
    async fn get_info_no_clients_returns_empty_connected_clients() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("1".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert!(json["uptime_seconds"].is_number());
        let clients = json["connected_clients"].as_array().unwrap();
        assert!(clients.is_empty());
        assert_eq!(json["bandwidth"]["outbound_bytes_total"], 0u64);
        assert_eq!(json["bandwidth"]["peak_outbound_bytes_per_second"], 0u64);
        assert!(json["available_platforms"].is_array());
    }

    #[tokio::test]
    async fn get_info_with_connected_client_returns_telemetry() {
        let ctx = make_registered_ctx(false, false).await;
        ctx.server_info
            .register(ctx.client.id, Arc::clone(&ctx.client))
            .await;

        let req = WsEnvelope {
            id: Some("1".to_owned()),
            inner: WsRequest::GetInfo,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let clients = json["connected_clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert!(clients[0]["client_id"].as_str().unwrap().starts_with("ws_"));
        assert!(clients[0]["bytes_sent"].is_number());
        assert!(clients[0]["uptime_seconds"].is_number());
        assert!(clients[0]["events_per_second"].is_number());
        assert!(clients[0]["subscriptions"].is_array());
    }

    fn sample_command() -> forge_types::Command {
        forge_types::Command {
            id: forge_types::CommandId::new(),
            action_id: ActionId::new(),
            name: "!quote".to_string(),
            cooldown_secs: 30,
            permission: forge_types::CommandPermission::Everyone,
        }
    }

    fn sample_global_entry() -> GlobalEntry {
        GlobalEntry {
            name: "counter".to_string(),
            value: forge_types::Variant::Int(42),
            persisted: true,
            reads: 5,
            writes: 2,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_001_000).unwrap(),
        }
    }

    #[tokio::test]
    async fn get_commands_returns_list_with_sample_command() {
        let cmd = sample_command();
        let cmd_id = cmd.id.to_string();
        let action_id = cmd.action_id.to_string();
        let dp = VecCommandDp::with_commands(vec![cmd]);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("7".to_owned()),
            inner: WsRequest::GetCommands,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let cmds = json["commands"].as_array().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0]["id"].as_str().unwrap(), cmd_id);
        assert_eq!(cmds[0]["command"], "!quote");
        assert_eq!(cmds[0]["action_id"].as_str().unwrap(), action_id);
        assert_eq!(cmds[0]["cooldown_seconds"], 30u64);
        assert_eq!(cmds[0]["enabled"], true);
        assert_eq!(cmds[0]["permission_level"], "everyone");
    }

    #[tokio::test]
    async fn get_globals_returns_list_with_sample_global() {
        let entry = sample_global_entry();
        let dp = VecGlobalsDp::with_globals(vec![entry]);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("8".to_owned()),
            inner: WsRequest::GetGlobals,
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let globals = json["globals"].as_array().unwrap();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0]["key"], "counter");
        assert_eq!(globals[0]["value"], 42i64);
        assert_eq!(globals[0]["type"], "int");
        assert_eq!(globals[0]["persisted"], true);
        assert_eq!(globals[0]["reads"], 5u64);
        assert_eq!(globals[0]["writes"], 2u64);
        assert!(globals[0]["last_modified"].is_string());
    }

    #[tokio::test]
    async fn get_global_existing_returns_value() {
        let entry = sample_global_entry();
        let dp = VecGlobalsDp::with_globals(vec![entry]);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("9".to_owned()),
            inner: WsRequest::GetGlobal {
                name: "counter".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["name"], "counter");
        assert_eq!(json["value"], 42i64);
        assert_eq!(json["type"], "int");
    }

    #[tokio::test]
    async fn get_global_nonexistent_returns_not_found() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("9".to_owned()),
            inner: WsRequest::GetGlobal {
                name: "missing".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "NOT_FOUND"),
            other => panic!("expected NOT_FOUND error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_global_without_auth_returns_unauthenticated() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(99),
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
    async fn set_global_with_auth_returns_ok_and_emits_event() {
        let ctx = make_ctx(true, false);
        let mut bus_sub = ctx.bus.subscribe();
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::json!(99),
                persisted: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let event = bus_sub.recv().await.unwrap();
        assert_eq!(event.source, EventSource::Server);
        assert_eq!(event.kind, "global.set");
        assert_eq!(event.payload["name"], "counter");
        assert_eq!(event.payload["via"], "ws_api");
    }

    #[tokio::test]
    async fn set_global_null_value_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("10".to_owned()),
            inner: WsRequest::SetGlobal {
                name: "counter".to_owned(),
                value: serde_json::Value::Null,
                persisted: false,
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    fn sample_user_global_entry() -> forge_storage::UserGlobalEntry {
        forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "87654321".to_string(),
            name: "points".to_string(),
            value: forge_types::Variant::Int(500),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        }
    }

    #[tokio::test]
    async fn get_user_globals_empty_for_null_dp() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("11".to_owned()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert!(json["globals"].is_array());
        assert_eq!(json["globals"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_user_globals_with_user_id_returns_filtered_globals() {
        use crate::test_dp::VecUserGlobalsDp;
        let entry = sample_user_global_entry();
        let other_entry = forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "11111111".to_string(),
            name: "other".to_string(),
            value: forge_types::Variant::Int(0),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        };
        let dp = VecUserGlobalsDp::with_entries(vec![entry, other_entry]);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("11".to_owned()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: Some("87654321".to_owned()),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let globals = json["globals"].as_array().unwrap();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0]["user_id"], "87654321");
        assert_eq!(globals[0]["key"], "points");
        assert_eq!(globals[0]["value"], 500i64);
        assert_eq!(globals[0]["type"], "int");
        assert!(globals[0]["last_modified"].is_string());
    }

    #[tokio::test]
    async fn get_user_globals_without_user_id_returns_all_broadcaster_globals() {
        use crate::test_dp::VecUserGlobalsDp;
        let entry1 = sample_user_global_entry();
        let entry2 = forge_storage::UserGlobalEntry {
            broadcaster_id: "12345678".to_string(),
            user_id: "11111111".to_string(),
            name: "level".to_string(),
            value: forge_types::Variant::Int(5),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        };
        let dp = VecUserGlobalsDp::with_entries(vec![entry1, entry2]);
        let ctx = make_ctx_with_dp(false, dp);
        let req = WsEnvelope {
            id: Some("11".to_owned()),
            inner: WsRequest::GetUserGlobals {
                broadcaster_id: "12345678".to_owned(),
                user_id: None,
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        let globals = json["globals"].as_array().unwrap();
        assert_eq!(globals.len(), 2);
    }

    #[tokio::test]
    async fn trigger_code_event_without_auth_returns_unauthenticated() {
        let ctx = make_ctx(false, false);
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: "my_event".to_owned(),
                args: serde_json::json!({ "scene": "Gameplay" }),
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
    async fn trigger_code_event_with_auth_publishes_bus_event() {
        let ctx = make_ctx(true, false);
        let mut bus_sub = ctx.bus.subscribe();
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: "my_event".to_owned(),
                args: serde_json::json!({ "scene": "Gameplay" }),
            },
        };
        let resp = dispatch(req, &ctx).await;
        let json = serialize_response_frame(&resp);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["ok"], true);

        let event = bus_sub.recv().await.unwrap();
        assert_eq!(event.source, EventSource::Server);
        assert_eq!(event.kind, "custom.my_event");
        assert_eq!(event.payload["scene"], "Gameplay");
    }

    #[tokio::test]
    async fn trigger_code_event_invalid_name_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: "Invalid-Name!".to_owned(),
                args: serde_json::json!({}),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn trigger_code_event_empty_name_returns_invalid_payload() {
        let ctx = make_ctx(true, false);
        let req = WsEnvelope {
            id: Some("12".to_owned()),
            inner: WsRequest::TriggerCodeEvent {
                name: String::new(),
                args: serde_json::json!({}),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "INVALID_PAYLOAD"),
            other => panic!("expected INVALID_PAYLOAD error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn authenticate_correct_token_marks_session_authenticated() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a1".to_owned()),
            inner: WsRequest::Auth {
                token: "test-token".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Ok(data) => {
                assert_eq!(data.get("authenticated"), Some(&serde_json::json!(true)));
            }
            other => panic!("expected ok response, got {other:?}"),
        }
        assert!(ctx.client.authenticated.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn authenticate_wrong_token_returns_auth_failed() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a2".to_owned()),
            inner: WsRequest::Auth {
                token: "wrong-token".to_owned(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED error, got {other:?}"),
        }
        assert!(!ctx.client.authenticated.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn authenticate_empty_token_returns_auth_failed() {
        let ctx = make_ctx(false, true);
        let req = WsEnvelope {
            id: Some("a3".to_owned()),
            inner: WsRequest::Auth {
                token: String::new(),
            },
        };
        let resp = dispatch(req, &ctx).await;
        match resp.inner {
            WsResponse::Error {
                code: Some(code), ..
            } => assert_eq!(code, "AUTH_FAILED"),
            other => panic!("expected AUTH_FAILED error, got {other:?}"),
        }
        assert!(!ctx.client.authenticated.load(Ordering::Acquire));
    }
}
