use forge_events::{CUSTOM_EVENT_PREFIX, Event, EventSource, GlobalSet, TypedEvent};
use forge_runtime::{ExecutionRequest, RuntimeError};
use forge_storage::{GlobalsRepo, UserGlobalsRepo};
use forge_types::{EventId, PlatformId, Variant};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::context::DispatchContext;
use super::envelope::WsResponse;
use super::helpers::{
    MAX_CODE_EVENT_NAME_LEN, build_arg_stack, parse_id, valid_code_event_name,
    variant_to_wire_value,
};
use super::introspect::{build_connected_accounts, build_connected_clients};

const WS_API_WRITER: &str = "ws_api";

const DEFAULT_EVENTS_LIMIT: u32 = 100;
const MAX_EVENTS_LIMIT: u32 = 500;

fn global_wire_row(
    value: &Variant,
    last_modified: Option<OffsetDateTime>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut row = serde_json::Map::new();
    row.insert("value".to_owned(), variant_to_wire_value(value));
    row.insert("type".to_owned(), value.type_tag().to_string().into());
    if let Some(at) = last_modified {
        row.insert(
            "last_modified".to_owned(),
            at.format(&Rfc3339).unwrap_or_default().into(),
        );
    }
    row
}

pub(crate) async fn handle_get_info(ctx: &DispatchContext) -> WsResponse {
    let connected_accounts = build_connected_accounts(ctx.credentials.as_ref()).await;
    let connected_clients = build_connected_clients(&ctx.bus_adapter).await;
    let bw = &ctx.server_info.bandwidth;
    WsResponse::Ok(serde_json::json!({
        "version": ctx.server_info.version,
        "uptime_seconds": ctx.server_info.uptime_seconds(),
        "connected_accounts": connected_accounts,
        "available_platforms": ctx.server_info.available_platforms,
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
            let mut row = global_wire_row(&g.value, Some(g.last_modified));
            row.insert("key".to_owned(), g.name.clone().into());
            row.insert("persisted".to_owned(), g.persisted.into());
            row.insert("reads".to_owned(), g.reads.into());
            row.insert("writes".to_owned(), g.writes.into());
            serde_json::Value::Object(row)
        })
        .collect();
    WsResponse::Ok(serde_json::json!({ "globals": wire_globals }))
}

pub(crate) async fn handle_get_global(name: String, ctx: &DispatchContext) -> WsResponse {
    let globals_repo: &dyn GlobalsRepo = ctx.globals.as_ref();
    match globals_repo.get(&name).await {
        Ok(Some(value)) => {
            let mut row = global_wire_row(&value, None);
            row.insert("name".to_owned(), name.into());
            WsResponse::Ok(serde_json::Value::Object(row))
        }
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
    let variant = match Variant::from_plain_json(value) {
        Ok(v) => v,
        Err(_) => {
            return WsResponse::Error {
                code: Some("INVALID_PAYLOAD".to_owned()),
                message: "value must be a non-null JSON scalar, array, or object".to_owned(),
            };
        }
    };
    let globals_repo: &dyn GlobalsRepo = ctx.globals.as_ref();
    let prev_value = globals_repo
        .get(&name)
        .await
        .inspect_err(
            |error| tracing::warn!(%error, global = %name, "previous global value unreadable"),
        )
        .ok()
        .flatten();
    let changed = GlobalSet {
        key: name.clone(),
        new_value: variant_to_wire_value(&variant),
        prev_value: prev_value
            .as_ref()
            .map(variant_to_wire_value)
            .unwrap_or(serde_json::Value::Null),
        persisted: Some(persisted),
        via: Some(WS_API_WRITER.to_owned()),
    };
    match globals_repo.set(&name, variant, persisted).await {
        Ok(()) => {
            ctx.bus
                .publish(changed.into_event(EventSource::Server, None));
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
                    let mut row = global_wire_row(&e.value, Some(e.last_modified));
                    row.insert("user_id".to_owned(), e.user_id.clone().into());
                    row.insert("key".to_owned(), e.name.clone().into());
                    serde_json::Value::Object(row)
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
            message: format!("name must match [a-z0-9_]{{1,{MAX_CODE_EVENT_NAME_LEN}}}"),
        };
    }
    let kind = format!("{CUSTOM_EVENT_PREFIX}{name}");
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
    let aid = match parse_id(&action_id, "invalid action id format") {
        Ok(id) => id,
        Err(refusal) => return refusal,
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

    let initial_args = build_arg_stack(args);

    match ctx
        .action_engine
        .dispatch(ExecutionRequest {
            action_id: aid,
            trigger_event_id,
            trigger_kind: None,
            manual_test: false,
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
    let limit = limit.unwrap_or(DEFAULT_EVENTS_LIMIT).min(MAX_EVENTS_LIMIT) as usize;
    let since_id: Option<EventId> = match since {
        None => None,
        Some(raw) => match parse_id::<EventId>(&raw, "since must be a valid event id") {
            Ok(id) => Some(id),
            Err(refusal) => return refusal,
        },
    };
    let events = ctx.bus.recent_since(limit, since_id).await;
    let wire: Vec<serde_json::Value> = events
        .iter()
        .filter_map(|event| {
            serde_json::to_value(event)
                .inspect_err(|error| tracing::warn!(%error, id = %event.id, "event dropped from the response"))
                .ok()
        })
        .collect();
    WsResponse::Ok(serde_json::json!({ "events": wire }))
}

const ACTIVE_VIEWER_WINDOW_SECS: i64 = 300;
const ACTIVE_VIEWER_LOOKBACK_LIMIT: usize = 500;

pub(crate) async fn handle_get_active_viewers(ctx: &DispatchContext) -> WsResponse {
    let events = ctx
        .bus
        .recent_since(ACTIVE_VIEWER_LOOKBACK_LIMIT, None)
        .await;
    let wire = active_viewers(&events, OffsetDateTime::now_utc());
    WsResponse::Ok(serde_json::json!({ "viewers": wire }))
}

fn active_viewers(events: &[Event], now: OffsetDateTime) -> Vec<serde_json::Value> {
    use std::collections::BTreeMap;

    let cutoff = now - time::Duration::seconds(ACTIVE_VIEWER_WINDOW_SECS);
    let mut viewers: BTreeMap<(PlatformId, String), serde_json::Value> = BTreeMap::new();

    for ev in events {
        if ev.timestamp < cutoff {
            continue;
        }
        let Some(platform) = ev.source.to_platform_id() else {
            continue;
        };
        let Some(viewer) = forge_types::ChatViewer::read(&ev.payload) else {
            continue;
        };
        let profile = forge_types::ChatViewerProfile::read(&ev.payload);
        let login = if profile.login.is_empty() {
            viewer.name.clone()
        } else {
            profile.login
        };
        let roles = profile.roles;

        viewers
            .entry((platform, viewer.id.clone()))
            .or_insert_with(|| {
                serde_json::json!({
                    "platform": platform.as_str(),
                    "login": login,
                    "id": viewer.id,
                    "roles": roles,
                })
            });
    }

    viewers.into_values().collect()
}

const MAX_OVERLAY_FILE_ENTRIES: usize = 5_000;

pub(crate) async fn handle_get_overlay_files(recursive: bool, ctx: &DispatchContext) -> WsResponse {
    use std::path::{Path, PathBuf};

    let root: &Path = ctx.overlay_root.as_path();

    let canon_root = match tokio::fs::canonicalize(root).await {
        Ok(p) => p,
        Err(_) => {
            return WsResponse::Ok(serde_json::json!({ "files": [] }));
        }
    };

    let mut files: Vec<serde_json::Value> = Vec::new();
    let mut dirs_to_visit: Vec<PathBuf> = vec![canon_root.clone()];

    'walk: while let Some(dir) = dirs_to_visit.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        loop {
            let entry = match read_dir.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!(%error, dir = %dir.display(), "overlay file listing stopped early");
                    break;
                }
            };
            if files.len() >= MAX_OVERLAY_FILE_ENTRIES {
                break 'walk;
            }
            let canon_entry = match crate::sandbox::confine_within(&canon_root, &entry.path()).await
            {
                Some(p) => p,
                None => continue,
            };
            let rel = match canon_entry.strip_prefix(&canon_root) {
                Ok(r) => r.to_owned(),
                Err(_) => continue,
            };
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
                let mime = crate::media_type::mime_for_extension(ext)
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

    WsResponse::Ok(serde_json::json!({ "files": files }))
}

pub(crate) async fn handle_replay_event(event_id: String, ctx: &DispatchContext) -> WsResponse {
    let eid = match parse_id(&event_id, "invalid event id format") {
        Ok(id) => id,
        Err(refusal) => return refusal,
    };
    match ctx.bus.replay_and_publish(eid).await {
        Ok(()) => WsResponse::Ok(serde_json::json!({ "ok": true })),
        Err(RuntimeError::EventNotFound(_)) => WsResponse::Error {
            code: Some("NOT_FOUND".to_owned()),
            message: "event not found".to_owned(),
        },
        Err(error) => WsResponse::Error {
            code: Some("RUNTIME_ERROR".to_owned()),
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use forge_types::ChatViewer;
    use time::Duration;

    use super::*;

    const WINDOW: Duration = Duration::seconds(ACTIVE_VIEWER_WINDOW_SECS);

    fn now() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    fn chat_at(viewer_id: &str, at: OffsetDateTime) -> Event {
        let mut payload = serde_json::json!({});
        ChatViewer::new(viewer_id, viewer_id).attach(&mut payload);
        let mut event = Event::new(EventSource::Twitch, "chat.message", payload);
        event.timestamp = at;
        event
    }

    #[test]
    fn a_viewer_counts_as_active_up_to_the_window_edge_and_not_past_it() {
        let events = [
            chat_at("on_the_edge", now() - WINDOW),
            chat_at("just_expired", now() - WINDOW - Duration::seconds(1)),
        ];

        let viewers = active_viewers(&events, now());

        let ids: Vec<&str> = viewers.iter().filter_map(|v| v["id"].as_str()).collect();
        assert_eq!(ids, ["on_the_edge"]);
    }

    #[test]
    fn a_global_row_carries_the_timestamp_only_when_one_is_given() {
        let stamped = global_wire_row(&Variant::Int(5), Some(OffsetDateTime::UNIX_EPOCH));
        assert_eq!(stamped["value"], 5);
        assert_eq!(stamped["type"], "int");
        assert_eq!(stamped["last_modified"], "1970-01-01T00:00:00Z");

        let bare = global_wire_row(&Variant::Int(5), None);
        assert!(!bare.contains_key("last_modified"));
    }
}
