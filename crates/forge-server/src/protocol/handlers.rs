use forge_events::{Event, EventSource};
use forge_runtime::{BusError, ExecutionRequest};
use forge_storage::{GlobalsRepo, UserGlobalsRepo};
use forge_types::{ActionId, EventId};

use super::context::DispatchContext;
use super::envelope::WsResponse;
use super::helpers::{build_arg_stack, valid_code_event_name, variant_to_wire_value};
use super::introspect::{build_connected_accounts, build_connected_clients};

pub(crate) fn mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => Some("text/html"),
        "js" | "mjs" => Some("application/javascript"),
        "css" => Some("text/css"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "svg" => Some("image/svg+xml"),
        "wav" => Some("audio/wav"),
        "mp3" => Some("audio/mpeg"),
        _ => None,
    }
}

pub(crate) async fn handle_get_info(ctx: &DispatchContext) -> WsResponse {
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

pub(crate) async fn handle_get_globals(ctx: &DispatchContext) -> WsResponse {
    let globals_repo: &dyn GlobalsRepo = ctx.globals.as_ref();
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

pub(crate) async fn handle_get_global(name: String, ctx: &DispatchContext) -> WsResponse {
    let globals_repo: &dyn GlobalsRepo = ctx.globals.as_ref();
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

pub(crate) async fn handle_set_global(
    name: String,
    value: serde_json::Value,
    persisted: bool,
    ctx: &DispatchContext,
) -> WsResponse {
    let variant = match forge_types::Variant::from_json(value) {
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
    let globals_repo: &dyn GlobalsRepo = ctx.globals.as_ref();
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

pub(crate) async fn handle_get_user_globals(
    broadcaster_id: String,
    user_id: Option<String>,
    ctx: &DispatchContext,
) -> WsResponse {
    let user_globals_repo: &dyn UserGlobalsRepo = ctx.user_globals.as_ref();
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

pub(crate) async fn handle_trigger_code_event(
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

pub(crate) async fn handle_get_actions(ctx: &DispatchContext) -> WsResponse {
    let actions = match ctx.actions.list().await {
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

pub(crate) async fn handle_do_action(
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

    match ctx.actions.get(aid).await {
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

pub(crate) async fn handle_get_events(
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

const ACTIVE_VIEWER_WINDOW_SECS: i64 = 300;
const ACTIVE_VIEWER_LOOKBACK_LIMIT: usize = 500;

pub(crate) async fn handle_get_active_viewers(ctx: &DispatchContext) -> WsResponse {
    use std::collections::BTreeMap;

    let events = ctx
        .bus
        .recent_since(ACTIVE_VIEWER_LOOKBACK_LIMIT, None)
        .await;
    let cutoff =
        time::OffsetDateTime::now_utc() - time::Duration::seconds(ACTIVE_VIEWER_WINDOW_SECS);

    let mut viewers: BTreeMap<(&'static str, String), serde_json::Value> = BTreeMap::new();

    for ev in events.iter() {
        if ev.kind != "chat.message" {
            continue;
        }
        if ev.timestamp < cutoff {
            continue;
        }
        let platform = match ev.source {
            EventSource::Twitch => "twitch",
            EventSource::YouTube => "youtube",
            EventSource::Kick => "kick",
            EventSource::Trovo => "trovo",
            _ => continue,
        };
        let user = match ev.payload.get("user") {
            Some(u) if u.is_object() => u,
            _ => continue,
        };
        let login = user
            .get("login")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let id = user.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        if login.is_empty() || id.is_empty() {
            continue;
        }
        let roles: Vec<String> = user
            .get("roles")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        viewers.entry((platform, id.to_owned())).or_insert_with(|| {
            serde_json::json!({
                "platform": platform,
                "login": login,
                "id": id,
                "roles": roles,
            })
        });
    }

    let wire: Vec<serde_json::Value> = viewers.into_values().collect();
    WsResponse::Ok(serde_json::json!({ "viewers": wire }))
}

pub(crate) async fn handle_get_overlay_files(recursive: bool, ctx: &DispatchContext) -> WsResponse {
    use std::path::{Path, PathBuf};

    let root: &Path = ctx.overlay_root.as_path();
    let root_path_str = root.to_string_lossy().into_owned();

    let canon_root = match tokio::fs::canonicalize(root).await {
        Ok(p) => p,
        Err(_) => {
            return WsResponse::Ok(serde_json::json!({
                "root_path": root_path_str,
                "files": [],
            }));
        }
    };

    let mut files: Vec<serde_json::Value> = Vec::new();
    let mut dirs_to_visit: Vec<PathBuf> = vec![canon_root.clone()];

    while let Some(dir) = dirs_to_visit.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let canon_entry = match tokio::fs::canonicalize(entry.path()).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !canon_entry.starts_with(&canon_root) {
                continue;
            }
            let rel = match canon_entry.strip_prefix(&canon_root) {
                Ok(r) => r.to_owned(),
                Err(_) => continue,
            };
            if rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                continue;
            }
            let meta = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let is_dir = meta.is_dir();
            if is_dir && recursive {
                dirs_to_visit.push(canon_entry.clone());
            }
            let (kind, size_bytes, mime) = if is_dir {
                ("dir", 0u64, serde_json::Value::Null)
            } else {
                let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or_default();
                let mime = mime_for_extension(ext)
                    .map(|s| serde_json::Value::String(s.to_owned()))
                    .unwrap_or(serde_json::Value::Null);
                ("file", meta.len(), mime)
            };
            files.push(serde_json::json!({
                "name": rel.to_string_lossy(),
                "kind": kind,
                "size_bytes": size_bytes,
                "mime": mime,
            }));
        }
    }

    WsResponse::Ok(serde_json::json!({
        "root_path": root_path_str,
        "files": files,
    }))
}

pub(crate) async fn handle_replay_event(event_id: String, ctx: &DispatchContext) -> WsResponse {
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
