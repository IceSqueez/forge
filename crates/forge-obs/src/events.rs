use std::collections::HashMap;

use forge_events::{Event, EventSource};
use forge_platform_core::{HealthDelta, HealthValue};
use forge_types::EventId;
use serde_json::json;

use crate::catalog::ObsCatalog;
use crate::health::HealthSnapshot;

pub(crate) fn make_connection_connected() -> Event {
    Event::new(EventSource::Obs, "connection.connected", json!({}))
}

pub(crate) fn make_connection_disconnected(reason: &str) -> Event {
    Event::new(
        EventSource::Obs,
        "connection.disconnected",
        json!({ "reason": reason }),
    )
}

pub(crate) fn make_connection_auth_failed(message: &str) -> Event {
    Event::new(
        EventSource::Obs,
        "connection.auth_failed",
        json!({ "error_message": message }),
    )
}

pub(crate) fn make_scene_changed_event(
    from_scene: Option<&str>,
    to_scene: &str,
    cause: Option<EventId>,
) -> Event {
    let payload = json!({
        "from_scene": from_scene.unwrap_or(""),
        "to_scene": to_scene,
    });
    match cause {
        Some(c) => Event::caused_by(EventSource::Obs, "scene.changed", payload, c),
        None => Event::new(EventSource::Obs, "scene.changed", payload),
    }
}

pub(crate) fn make_record_event(active: bool, path: Option<&str>) -> Event {
    let kind = if active {
        "recording.started"
    } else {
        "recording.stopped"
    };
    Event::new(EventSource::Obs, kind, json!({ "output_path": path }))
}

pub(crate) fn make_record_state_event(
    active: bool,
    state: &obws::events::OutputState,
    path: Option<&str>,
) -> Event {
    use obws::events::OutputState;
    let kind = match state {
        OutputState::Starting => "recording.starting",
        OutputState::Stopping => "recording.stopping",
        OutputState::Paused => "recording.paused",
        OutputState::Resumed => "recording.resumed",
        _ => {
            if active {
                "recording.started"
            } else {
                "recording.stopped"
            }
        }
    };
    let state_str = match state {
        OutputState::Starting => "starting",
        OutputState::Started => "started",
        OutputState::Stopping => "stopping",
        OutputState::Stopped => "stopped",
        OutputState::Paused => "paused",
        OutputState::Resumed => "resumed",
        _ => "unknown",
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ "output_state": state_str, "is_active": active, "output_path": path }),
    )
}

pub(crate) fn make_virtualcam_event(active: bool, state: &obws::events::OutputState) -> Event {
    use obws::events::OutputState;
    let kind = match state {
        OutputState::Starting => "virtualcam.starting",
        OutputState::Started => "virtualcam.started",
        OutputState::Stopping => "virtualcam.stopping",
        OutputState::Stopped => "virtualcam.stopped",
        _ => {
            if active {
                "virtualcam.started"
            } else {
                "virtualcam.stopped"
            }
        }
    };
    let state_str = match state {
        OutputState::Starting => "starting",
        OutputState::Started => "started",
        OutputState::Stopping => "stopping",
        OutputState::Stopped => "stopped",
        OutputState::Paused => "paused",
        OutputState::Resumed => "resumed",
        _ => "unknown",
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ "output_state": state_str, "is_active": active }),
    )
}

pub(crate) fn make_stream_event(active: bool, state: &obws::events::OutputState) -> Event {
    use obws::events::OutputState;
    let kind = match state {
        OutputState::Starting => "streaming.starting",
        OutputState::Started => "streaming.started",
        OutputState::Stopping => "streaming.stopping",
        OutputState::Stopped => "streaming.stopped",
        OutputState::Reconnecting => "streaming.reconnecting",
        OutputState::Reconnected => "streaming.reconnected",
        _ => {
            if active {
                "streaming.started"
            } else {
                "streaming.stopped"
            }
        }
    };
    let state_str = match state {
        OutputState::Starting => "starting",
        OutputState::Started => "started",
        OutputState::Stopping => "stopping",
        OutputState::Stopped => "stopped",
        OutputState::Reconnecting => "reconnecting",
        OutputState::Reconnected => "reconnected",
        OutputState::Paused => "paused",
        OutputState::Resumed => "resumed",
        _ => "unknown",
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ "output_state": state_str, "is_active": active }),
    )
}

pub(crate) fn map_obs_event(
    ev: &obws::events::Event,
    from_scene: Option<&str>,
    cause: Option<EventId>,
) -> Option<Event> {
    match ev {
        obws::events::Event::CurrentProgramSceneChanged { id } => {
            Some(make_scene_changed_event(from_scene, &id.name, cause))
        }
        obws::events::Event::CurrentPreviewSceneChanged { id } => Some(Event::new(
            EventSource::Obs,
            "scene.preview_changed",
            json!({
                "name_old": from_scene.unwrap_or(""),
                "name_new": id.name,
            }),
        )),
        obws::events::Event::SceneListChanged { scenes } => {
            let names: Vec<&str> = scenes.iter().map(|s| s.name.as_str()).collect();
            Some(Event::new(
                EventSource::Obs,
                "scene.list_changed",
                json!({ "all_names": names }),
            ))
        }
        obws::events::Event::SceneCreated { id, .. } => Some(Event::new(
            EventSource::Obs,
            "scene.created",
            json!({ "scene_name": id.name }),
        )),
        obws::events::Event::SceneRemoved { id, .. } => Some(Event::new(
            EventSource::Obs,
            "scene.removed",
            json!({ "scene_name": id.name }),
        )),
        obws::events::Event::SceneNameChanged {
            old_name, new_name, ..
        } => Some(Event::new(
            EventSource::Obs,
            "scene.renamed",
            json!({ "scene_name_old": old_name, "scene_name_new": new_name }),
        )),
        obws::events::Event::CurrentProfileChanged { name } => Some(Event::new(
            EventSource::Obs,
            "profile.current_changed",
            json!({ "profile_name": name }),
        )),
        obws::events::Event::ProfileListChanged { profiles } => Some(Event::new(
            EventSource::Obs,
            "profile.list_changed",
            json!({ "all_names": profiles }),
        )),
        obws::events::Event::SceneCollectionListChanged { collections } => Some(Event::new(
            EventSource::Obs,
            "collection.list_changed",
            json!({ "all_names": collections }),
        )),
        obws::events::Event::CurrentSceneCollectionChanging { name } => Some(Event::new(
            EventSource::Obs,
            "collection.changing",
            json!({ "name": name }),
        )),
        obws::events::Event::CurrentSceneCollectionChanged { name } => Some(Event::new(
            EventSource::Obs,
            "collection.changed",
            json!({ "name": name }),
        )),
        obws::events::Event::RecordStateChanged {
            active,
            state,
            path,
        } => {
            use obws::events::OutputState;
            match state {
                OutputState::Started | OutputState::Stopped => {
                    Some(make_record_event(*active, path.as_deref()))
                }
                _ => Some(make_record_state_event(*active, state, path.as_deref())),
            }
        }
        obws::events::Event::RecordFileChanged { path } => Some(Event::new(
            EventSource::Obs,
            "recording.file_changed",
            json!({ "output_path_new": path }),
        )),
        obws::events::Event::StreamStateChanged { active, state } => {
            Some(make_stream_event(*active, state))
        }
        obws::events::Event::VirtualcamStateChanged { active, state } => {
            Some(make_virtualcam_event(*active, state))
        }
        obws::events::Event::StudioModeStateChanged { enabled } => {
            let kind = if *enabled {
                "studio.enabled"
            } else {
                "studio.disabled"
            };
            Some(Event::new(
                EventSource::Obs,
                kind,
                json!({ "enabled": enabled }),
            ))
        }
        obws::events::Event::SceneTransitionStarted { id } => Some(Event::new(
            EventSource::Obs,
            "transition.started",
            json!({ "transition_name": id.name }),
        )),
        obws::events::Event::SceneTransitionEnded { id } => Some(Event::new(
            EventSource::Obs,
            "transition.ended",
            json!({ "transition_name": id.name }),
        )),
        obws::events::Event::SceneTransitionVideoEnded { id } => Some(Event::new(
            EventSource::Obs,
            "transition.video_ended",
            json!({ "transition_name": id.name }),
        )),
        obws::events::Event::InputMuteStateChanged { id, muted } => Some(Event::new(
            EventSource::Obs,
            "audio.source_mute_changed",
            json!({ "source_name": id.name, "is_muted": muted }),
        )),
        obws::events::Event::InputVolumeChanged { id, mul, db } => Some(Event::new(
            EventSource::Obs,
            "audio.source_volume_changed",
            json!({ "source_name": id.name, "volume_db": db, "volume_multiplier": mul }),
        )),
        obws::events::Event::InputAudioBalanceChanged { id, audio_balance } => Some(Event::new(
            EventSource::Obs,
            "audio.source_balance_changed",
            json!({ "source_name": id.name, "balance": audio_balance }),
        )),
        obws::events::Event::InputAudioSyncOffsetChanged { id, offset } => Some(Event::new(
            EventSource::Obs,
            "audio.source_sync_offset_changed",
            json!({ "source_name": id.name, "sync_offset_ms": offset.whole_milliseconds() }),
        )),
        obws::events::Event::InputCreated {
            id,
            unversioned_kind,
            ..
        } => Some(Event::new(
            EventSource::Obs,
            "source.input_created",
            json!({ "source_name": id.name, "source_kind": unversioned_kind }),
        )),
        obws::events::Event::InputRemoved { id } => Some(Event::new(
            EventSource::Obs,
            "source.input_removed",
            json!({ "source_name": id.name }),
        )),
        obws::events::Event::InputNameChanged {
            old_name, new_name, ..
        } => Some(Event::new(
            EventSource::Obs,
            "source.input_renamed",
            json!({ "source_name_old": old_name, "source_name_new": new_name }),
        )),
        obws::events::Event::SourceFilterCreated {
            source,
            filter,
            kind,
            ..
        } => Some(Event::new(
            EventSource::Obs,
            "filter.created",
            json!({ "source_name": source, "filter_name": filter, "filter_kind": kind }),
        )),
        obws::events::Event::SourceFilterRemoved { source, filter } => Some(Event::new(
            EventSource::Obs,
            "filter.removed",
            json!({ "source_name": source, "filter_name": filter }),
        )),
        obws::events::Event::SourceFilterEnableStateChanged {
            source,
            filter,
            enabled,
        } => Some(Event::new(
            EventSource::Obs,
            "filter.enabled_changed",
            json!({ "source_name": source, "filter_name": filter, "is_enabled": enabled }),
        )),
        _ => None,
    }
}

pub(crate) fn map_scene_item_visibility(scene: &str, source: &str, enabled: bool) -> Event {
    Event::new(
        EventSource::Obs,
        "source.visibility.changed",
        json!({ "scene": scene, "source": source, "visible": enabled }),
    )
}

pub(crate) fn map_scene_item_lock(scene: &str, source: &str, locked: bool) -> Event {
    Event::new(
        EventSource::Obs,
        "source.scene_item_lock_changed",
        json!({ "scene": scene, "source": source, "is_locked": locked }),
    )
}

pub(crate) fn resolve_source_name(
    cache: &HashMap<(String, String), i64>,
    scene: &str,
    item_id: u64,
) -> Option<String> {
    let target = item_id as i64;
    cache
        .iter()
        .find(|((s, _), id)| s.as_str() == scene && **id == target)
        .map(|((_, source), _)| source.clone())
}

pub(crate) fn apply_catalog_update(ev: &obws::events::Event, catalog: &mut ObsCatalog) {
    match ev {
        obws::events::Event::CurrentProgramSceneChanged { id } => {
            catalog.current_scene = Some(id.name.clone());
        }
        obws::events::Event::CurrentPreviewSceneChanged { id } => {
            catalog.current_preview_scene = Some(id.name.clone());
        }
        obws::events::Event::SceneListChanged { scenes } => {
            catalog.scenes = scenes.iter().map(|s| s.name.clone()).collect();
        }
        obws::events::Event::SceneCreated { id, .. } if !catalog.scenes.contains(&id.name) => {
            catalog.scenes.push(id.name.clone());
        }
        obws::events::Event::SceneRemoved { id, .. } => {
            catalog.scenes.retain(|s| s != &id.name);
            catalog.sources.remove(id.name.as_str());
        }
        obws::events::Event::SceneNameChanged {
            old_name, new_name, ..
        } => {
            for scene in &mut catalog.scenes {
                if scene == old_name {
                    *scene = new_name.clone();
                }
            }
            if catalog.current_scene.as_deref() == Some(old_name.as_str()) {
                catalog.current_scene = Some(new_name.clone());
            }
            if let Some(sources) = catalog.sources.remove(old_name.as_str()) {
                catalog.sources.insert(new_name.clone(), sources);
            }
        }
        _ => {}
    }
}

/// Shared by the event-driven update and the cold-connect seed so both render the same shape.
pub(crate) fn make_stream_health_value(active: bool) -> HealthValue {
    HealthValue::Status {
        label: if active {
            "Live".to_owned()
        } else {
            "Offline".to_owned()
        },
        active,
        detail: None,
    }
}

pub(crate) fn make_record_health_value(active: bool) -> HealthValue {
    HealthValue::Status {
        label: if active {
            "Active".to_owned()
        } else {
            "Off".to_owned()
        },
        active,
        detail: None,
    }
}

pub(crate) fn apply_health_update(
    ev: &obws::events::Event,
    snapshot: &mut HealthSnapshot,
) -> Vec<HealthDelta> {
    match ev {
        obws::events::Event::StreamStateChanged { active, .. } => {
            if snapshot.stream_active == *active {
                return vec![];
            }
            snapshot.stream_active = *active;
            vec![HealthDelta {
                index: 0,
                new_value: make_stream_health_value(*active),
            }]
        }
        obws::events::Event::RecordStateChanged { active, .. } => {
            if snapshot.record_active == *active {
                return vec![];
            }
            snapshot.record_active = *active;
            vec![HealthDelta {
                index: 1,
                new_value: make_record_health_value(*active),
            }]
        }
        _ => vec![],
    }
}

/// Only emits a delta when the rendered value actually changed.
pub(crate) fn apply_stats_update(
    stats: &obws::responses::general::Stats,
    snapshot: &mut HealthSnapshot,
) -> Vec<HealthDelta> {
    let mut deltas = Vec::new();

    if (snapshot.cpu_percent - stats.cpu_usage).abs() > f64::EPSILON
        || (snapshot.fps - stats.active_fps).abs() > f64::EPSILON
    {
        snapshot.cpu_percent = stats.cpu_usage;
        snapshot.fps = stats.active_fps;
        deltas.push(HealthDelta {
            index: 2,
            new_value: HealthValue::Pair {
                left: format!("{:.1}%", stats.cpu_usage),
                right: format!("{:.1} fps", stats.active_fps),
            },
        });
    }

    let dropped = u64::from(stats.output_skipped_frames);
    let total = u64::from(stats.output_total_frames);
    if snapshot.dropped_frames != dropped || snapshot.total_frames != total {
        snapshot.dropped_frames = dropped;
        snapshot.total_frames = total;
        deltas.push(HealthDelta {
            index: 3,
            new_value: HealthValue::Ratio {
                used: dropped,
                total,
                reset_hint: None,
            },
        });
    }

    deltas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_scene_changed_event_emits_from_and_to_fields() {
        let ev = make_scene_changed_event(Some("Menu"), "Gameplay", None);
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "scene.changed");
        assert_eq!(ev.payload["from_scene"], "Menu");
        assert_eq!(ev.payload["to_scene"], "Gameplay");
        assert!(ev.caused_by.is_none());
    }

    #[test]
    fn make_scene_changed_event_unknown_from_scene_uses_empty_string() {
        let ev = make_scene_changed_event(None, "Gameplay", None);
        assert_eq!(ev.payload["from_scene"], "");
        assert_eq!(ev.payload["to_scene"], "Gameplay");
    }

    #[test]
    fn make_scene_changed_event_with_cause_populates_caused_by() {
        let cause = EventId::new();
        let ev = make_scene_changed_event(Some("BRB"), "Gameplay", Some(cause));
        assert_eq!(ev.caused_by, Some(cause));
        assert_eq!(ev.payload["from_scene"], "BRB");
        assert_eq!(ev.payload["to_scene"], "Gameplay");
    }

    #[test]
    fn make_record_event_started_has_null_output_path() {
        let ev = make_record_event(true, None);
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "recording.started");
        assert_eq!(ev.payload["output_path"], serde_json::Value::Null);
    }

    #[test]
    fn make_record_event_stopped_includes_output_path() {
        let ev = make_record_event(false, Some("/home/user/recording.mkv"));
        assert_eq!(ev.kind, "recording.stopped");
        assert_eq!(ev.payload["output_path"], "/home/user/recording.mkv");
    }

    #[test]
    fn make_record_event_stopped_null_path_when_none() {
        let ev = make_record_event(false, None);
        assert_eq!(ev.payload["output_path"], serde_json::Value::Null);
    }

    #[test]
    fn make_connection_connected_emits_obs_kind_with_empty_payload() {
        let ev = make_connection_connected();
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "connection.connected");
        assert_eq!(ev.payload, json!({}));
    }

    #[test]
    fn make_connection_disconnected_carries_reason() {
        let ev = make_connection_disconnected("connection reset by peer");
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "connection.disconnected");
        assert_eq!(ev.payload["reason"], "connection reset by peer");
    }

    #[test]
    fn make_connection_disconnected_payload_has_no_disconnect_code_key() {
        let ev = make_connection_disconnected("closed");
        assert!(ev.payload.get("disconnect_code").is_none());
    }

    #[test]
    fn make_connection_auth_failed_carries_error_message() {
        let ev = make_connection_auth_failed("authentication rejected");
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "connection.auth_failed");
        assert_eq!(ev.payload["error_message"], "authentication rejected");
    }

    // The auth-failure message must never echo the OBS WebSocket password the user typed.
    #[test]
    fn make_connection_auth_failed_message_carries_nothing_password_like() {
        let ev = make_connection_auth_failed("authentication rejected");
        let lowered = ev.payload["error_message"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        assert!(!lowered.contains("password"));
        assert!(!lowered.contains("secret"));
    }

    #[test]
    fn map_scene_item_visibility_emits_obs_source_event() {
        let ev = map_scene_item_visibility("Gameplay", "Game Capture", true);
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "source.visibility.changed");
        assert_eq!(ev.payload["scene"], "Gameplay");
        assert_eq!(ev.payload["source"], "Game Capture");
        assert_eq!(ev.payload["visible"], true);
    }

    #[test]
    fn map_scene_item_visibility_hidden_source() {
        let ev = map_scene_item_visibility("BRB", "Webcam", false);
        assert_eq!(ev.payload["visible"], false);
        assert_eq!(ev.payload["scene"], "BRB");
    }

    #[test]
    fn map_scene_item_lock_emits_obs_source_event_for_both_states() {
        for locked in [true, false] {
            let ev = map_scene_item_lock("Gameplay", "Game Capture", locked);
            assert_eq!(ev.source, EventSource::Obs);
            assert_eq!(ev.kind, "source.scene_item_lock_changed");
            assert_eq!(ev.payload["scene"], "Gameplay");
            assert_eq!(ev.payload["source"], "Game Capture");
            assert_eq!(ev.payload["is_locked"], locked);
        }
    }

    #[test]
    fn resolve_source_name_finds_match_by_scene_and_id() {
        let mut cache = HashMap::new();
        cache.insert(("Gameplay".to_owned(), "Game Capture".to_owned()), 42i64);
        let result = resolve_source_name(&cache, "Gameplay", 42u64);
        assert_eq!(result.as_deref(), Some("Game Capture"));
    }

    #[test]
    fn resolve_source_name_returns_none_for_wrong_scene() {
        let mut cache = HashMap::new();
        cache.insert(("Gameplay".to_owned(), "Game Capture".to_owned()), 42i64);
        assert!(resolve_source_name(&cache, "BRB", 42u64).is_none());
    }

    #[test]
    fn resolve_source_name_returns_none_for_wrong_item_id() {
        let mut cache = HashMap::new();
        cache.insert(("Gameplay".to_owned(), "Game Capture".to_owned()), 42i64);
        assert!(resolve_source_name(&cache, "Gameplay", 99u64).is_none());
    }

    #[test]
    fn resolve_source_name_returns_none_for_empty_cache() {
        let cache: HashMap<(String, String), i64> = HashMap::new();
        assert!(resolve_source_name(&cache, "Gameplay", 1u64).is_none());
    }
}
