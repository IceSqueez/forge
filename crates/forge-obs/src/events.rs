use std::collections::HashMap;

use forge_events::{Event, EventSource};
use forge_platform_core::{HealthDelta, HealthValue};
use forge_types::EventId;
use serde_json::json;

use crate::catalog::ObsCatalog;
use crate::health::HealthSnapshot;
use crate::payload_fields::{
    audio as audio_fields, collection as collection_fields, connection as connection_fields,
    filter as filter_fields, profile as profile_fields, recording as recording_fields,
    scene as scene_fields, source as source_fields, streaming as streaming_fields,
    transition as transition_fields, virtualcam as virtualcam_fields,
};

pub(crate) fn make_connection_connected() -> Event {
    Event::new(EventSource::Obs, "obs.connection.connected", json!({}))
}

pub(crate) fn make_connection_disconnected(reason: &str, detail: Option<&str>) -> Event {
    Event::new(
        EventSource::Obs,
        "obs.connection.disconnected",
        json!({
            (connection_fields::REASON): reason,
            (connection_fields::DETAIL): detail,
        }),
    )
}

pub(crate) fn make_connection_auth_failed(message: &str) -> Event {
    Event::new(
        EventSource::Obs,
        "obs.connection.auth_failed",
        json!({ (connection_fields::ERROR_MESSAGE): message }),
    )
}

pub(crate) fn make_scene_changed_event(
    from_scene: Option<&str>,
    to_scene: &str,
    cause: Option<EventId>,
) -> Event {
    let payload = json!({
        (scene_fields::FROM_SCENE): from_scene.unwrap_or(""),
        (scene_fields::TO_SCENE): to_scene,
    });
    match cause {
        Some(c) => Event::caused_by(EventSource::Obs, "obs.scene.changed", payload, c),
        None => Event::new(EventSource::Obs, "obs.scene.changed", payload),
    }
}

pub(crate) fn make_record_event(active: bool, path: Option<&str>) -> Event {
    let kind = if active {
        "obs.recording.started"
    } else {
        "obs.recording.stopped"
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ (recording_fields::OUTPUT_PATH): path }),
    )
}

pub(crate) fn make_record_state_event(
    active: bool,
    state: &obws::events::OutputState,
    path: Option<&str>,
) -> Event {
    use obws::events::OutputState;
    let kind = match state {
        OutputState::Starting => "obs.recording.starting",
        OutputState::Stopping => "obs.recording.stopping",
        OutputState::Paused => "obs.recording.paused",
        OutputState::Resumed => "obs.recording.resumed",
        _ => {
            if active {
                "obs.recording.started"
            } else {
                "obs.recording.stopped"
            }
        }
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({
            (recording_fields::IS_ACTIVE): active,
            (recording_fields::OUTPUT_PATH): path,
        }),
    )
}

pub(crate) fn make_virtualcam_event(active: bool, state: &obws::events::OutputState) -> Event {
    use obws::events::OutputState;
    let kind = match state {
        OutputState::Starting => "obs.virtualcam.starting",
        OutputState::Started => "obs.virtualcam.started",
        OutputState::Stopping => "obs.virtualcam.stopping",
        OutputState::Stopped => "obs.virtualcam.stopped",
        _ => {
            if active {
                "obs.virtualcam.started"
            } else {
                "obs.virtualcam.stopped"
            }
        }
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ (virtualcam_fields::IS_ACTIVE): active }),
    )
}

pub(crate) fn make_stream_event(active: bool, state: &obws::events::OutputState) -> Event {
    use obws::events::OutputState;
    let kind = match state {
        OutputState::Starting => "obs.streaming.starting",
        OutputState::Started => "obs.streaming.started",
        OutputState::Stopping => "obs.streaming.stopping",
        OutputState::Stopped => "obs.streaming.stopped",
        OutputState::Reconnecting => "obs.streaming.reconnecting",
        OutputState::Reconnected => "obs.streaming.reconnected",
        _ => {
            if active {
                "obs.streaming.started"
            } else {
                "obs.streaming.stopped"
            }
        }
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ (streaming_fields::IS_ACTIVE): active }),
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
            "obs.scene.preview_changed",
            json!({
                (scene_fields::SCENE_NAME_OLD): from_scene.unwrap_or(""),
                (scene_fields::SCENE_NAME_NEW): id.name,
            }),
        )),
        obws::events::Event::SceneListChanged { scenes } => {
            let names: Vec<&str> = scenes.iter().map(|s| s.name.as_str()).collect();
            Some(Event::new(
                EventSource::Obs,
                "obs.scene.list_changed",
                json!({ (scene_fields::ALL_NAMES): names }),
            ))
        }
        obws::events::Event::SceneCreated { id, .. } => Some(Event::new(
            EventSource::Obs,
            "obs.scene.created",
            json!({ (scene_fields::SCENE_NAME): id.name }),
        )),
        obws::events::Event::SceneRemoved { id, .. } => Some(Event::new(
            EventSource::Obs,
            "obs.scene.removed",
            json!({ (scene_fields::SCENE_NAME): id.name }),
        )),
        obws::events::Event::SceneNameChanged {
            old_name, new_name, ..
        } => Some(Event::new(
            EventSource::Obs,
            "obs.scene.renamed",
            json!({
                (scene_fields::SCENE_NAME_OLD): old_name,
                (scene_fields::SCENE_NAME_NEW): new_name,
            }),
        )),
        obws::events::Event::CurrentProfileChanged { name } => Some(Event::new(
            EventSource::Obs,
            "obs.profile.current_changed",
            json!({ (profile_fields::PROFILE_NAME): name }),
        )),
        obws::events::Event::ProfileListChanged { profiles } => Some(Event::new(
            EventSource::Obs,
            "obs.profile.list_changed",
            json!({ (profile_fields::ALL_NAMES): profiles }),
        )),
        obws::events::Event::SceneCollectionListChanged { collections } => Some(Event::new(
            EventSource::Obs,
            "obs.collection.list_changed",
            json!({ (collection_fields::ALL_NAMES): collections }),
        )),
        obws::events::Event::CurrentSceneCollectionChanging { name } => Some(Event::new(
            EventSource::Obs,
            "obs.collection.changing",
            json!({ (collection_fields::COLLECTION_NAME): name }),
        )),
        obws::events::Event::CurrentSceneCollectionChanged { name } => Some(Event::new(
            EventSource::Obs,
            "obs.collection.changed",
            json!({ (collection_fields::COLLECTION_NAME): name }),
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
            "obs.recording.file_changed",
            json!({ (recording_fields::OUTPUT_PATH): path }),
        )),
        obws::events::Event::StreamStateChanged { active, state } => {
            Some(make_stream_event(*active, state))
        }
        obws::events::Event::VirtualcamStateChanged { active, state } => {
            Some(make_virtualcam_event(*active, state))
        }
        obws::events::Event::StudioModeStateChanged { enabled } => {
            let kind = if *enabled {
                "obs.studio.enabled"
            } else {
                "obs.studio.disabled"
            };
            Some(Event::new(EventSource::Obs, kind, json!({})))
        }
        obws::events::Event::SceneTransitionStarted { id } => Some(Event::new(
            EventSource::Obs,
            "obs.transition.started",
            json!({ (transition_fields::TRANSITION_NAME): id.name }),
        )),
        obws::events::Event::SceneTransitionEnded { id } => Some(Event::new(
            EventSource::Obs,
            "obs.transition.ended",
            json!({ (transition_fields::TRANSITION_NAME): id.name }),
        )),
        obws::events::Event::SceneTransitionVideoEnded { id } => Some(Event::new(
            EventSource::Obs,
            "obs.transition.video_ended",
            json!({ (transition_fields::TRANSITION_NAME): id.name }),
        )),
        obws::events::Event::InputMuteStateChanged { id, muted } => Some(Event::new(
            EventSource::Obs,
            "obs.audio.source_mute_changed",
            json!({ (audio_fields::SOURCE_NAME): id.name, (audio_fields::IS_MUTED): muted }),
        )),
        obws::events::Event::InputVolumeChanged { id, mul, db } => Some(Event::new(
            EventSource::Obs,
            "obs.audio.source_volume_changed",
            json!({
                (audio_fields::SOURCE_NAME): id.name,
                (audio_fields::VOLUME_DB): db,
                (audio_fields::VOLUME_MULTIPLIER): mul,
            }),
        )),
        obws::events::Event::InputAudioBalanceChanged { id, audio_balance } => Some(Event::new(
            EventSource::Obs,
            "obs.audio.source_balance_changed",
            json!({ (audio_fields::SOURCE_NAME): id.name, (audio_fields::BALANCE): audio_balance }),
        )),
        obws::events::Event::InputAudioSyncOffsetChanged { id, offset } => Some(Event::new(
            EventSource::Obs,
            "obs.audio.source_sync_offset_changed",
            json!({
                (audio_fields::SOURCE_NAME): id.name,
                (audio_fields::SYNC_OFFSET_MS): offset.whole_milliseconds(),
            }),
        )),
        obws::events::Event::InputCreated {
            id,
            unversioned_kind,
            ..
        } => Some(Event::new(
            EventSource::Obs,
            "obs.source.input_created",
            json!({
                (source_fields::SOURCE_NAME): id.name,
                (source_fields::SOURCE_KIND): unversioned_kind,
            }),
        )),
        obws::events::Event::InputRemoved { id } => Some(Event::new(
            EventSource::Obs,
            "obs.source.input_removed",
            json!({ (source_fields::SOURCE_NAME): id.name }),
        )),
        obws::events::Event::InputNameChanged {
            old_name, new_name, ..
        } => Some(Event::new(
            EventSource::Obs,
            "obs.source.input_renamed",
            json!({
                (source_fields::SOURCE_NAME_OLD): old_name,
                (source_fields::SOURCE_NAME_NEW): new_name,
            }),
        )),
        obws::events::Event::SourceFilterCreated {
            source,
            filter,
            kind,
            ..
        } => Some(Event::new(
            EventSource::Obs,
            "obs.filter.created",
            json!({
                (filter_fields::SOURCE_NAME): source,
                (filter_fields::FILTER_NAME): filter,
                (filter_fields::FILTER_KIND): kind,
            }),
        )),
        obws::events::Event::SourceFilterRemoved { source, filter } => Some(Event::new(
            EventSource::Obs,
            "obs.filter.removed",
            json!({
                (filter_fields::SOURCE_NAME): source,
                (filter_fields::FILTER_NAME): filter,
            }),
        )),
        obws::events::Event::SourceFilterEnableStateChanged {
            source,
            filter,
            enabled,
        } => Some(Event::new(
            EventSource::Obs,
            "obs.filter.enabled_changed",
            json!({
                (filter_fields::SOURCE_NAME): source,
                (filter_fields::FILTER_NAME): filter,
                (filter_fields::IS_ENABLED): enabled,
            }),
        )),
        _ => None,
    }
}

pub(crate) fn map_scene_item_visibility(scene: &str, source: &str, enabled: bool) -> Event {
    Event::new(
        EventSource::Obs,
        "obs.source.visibility_changed",
        json!({
            (source_fields::SCENE_NAME): scene,
            (source_fields::SOURCE_NAME): source,
            (source_fields::IS_VISIBLE): enabled,
        }),
    )
}

pub(crate) fn map_scene_item_lock(scene: &str, source: &str, locked: bool) -> Event {
    Event::new(
        EventSource::Obs,
        "obs.source.lock_changed",
        json!({
            (source_fields::SCENE_NAME): scene,
            (source_fields::SOURCE_NAME): source,
            (source_fields::IS_LOCKED): locked,
        }),
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
        assert_eq!(ev.kind, "obs.scene.changed");
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
        assert_eq!(ev.kind, "obs.recording.started");
        assert_eq!(ev.payload["output_path"], serde_json::Value::Null);
    }

    #[test]
    fn make_record_event_stopped_includes_output_path() {
        let ev = make_record_event(false, Some("/home/user/recording.mkv"));
        assert_eq!(ev.kind, "obs.recording.stopped");
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
        assert_eq!(ev.kind, "obs.connection.connected");
        assert_eq!(ev.payload, json!({}));
    }

    #[test]
    fn make_connection_disconnected_carries_reason() {
        let ev = make_connection_disconnected("connection reset by peer", None);
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "obs.connection.disconnected");
        assert_eq!(ev.payload["reason"], "connection reset by peer");
    }

    #[test]
    fn make_connection_disconnected_lost_uses_stable_reason_token_and_null_detail() {
        let ev = make_connection_disconnected(
            crate::payload_fields::connection::reason::CONNECTION_LOST,
            None,
        );
        assert_eq!(ev.payload["reason"], "connection_lost");
        assert_eq!(ev.payload["detail"], serde_json::Value::Null);
    }

    #[test]
    fn make_connection_disconnected_carries_detail_when_present() {
        let ev = make_connection_disconnected("connection_lost", Some("close code 4009"));
        assert_eq!(ev.payload["reason"], "connection_lost");
        assert_eq!(ev.payload["detail"], "close code 4009");
    }

    #[test]
    fn make_connection_auth_failed_carries_error_message() {
        let ev = make_connection_auth_failed("authentication rejected");
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "obs.connection.auth_failed");
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
        assert_eq!(ev.kind, "obs.source.visibility_changed");
        assert_eq!(ev.payload["scene_name"], "Gameplay");
        assert_eq!(ev.payload["source_name"], "Game Capture");
        assert_eq!(ev.payload["is_visible"], true);
    }

    #[test]
    fn map_scene_item_visibility_hidden_source() {
        let ev = map_scene_item_visibility("BRB", "Webcam", false);
        assert_eq!(ev.payload["is_visible"], false);
        assert_eq!(ev.payload["scene_name"], "BRB");
    }

    #[test]
    fn map_scene_item_lock_emits_obs_source_event_for_both_states() {
        for locked in [true, false] {
            let ev = map_scene_item_lock("Gameplay", "Game Capture", locked);
            assert_eq!(ev.source, EventSource::Obs);
            assert_eq!(ev.kind, "obs.source.lock_changed");
            assert_eq!(ev.payload["scene_name"], "Gameplay");
            assert_eq!(ev.payload["source_name"], "Game Capture");
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

    #[test]
    fn state_family_events_drop_redundant_output_state_field() {
        use obws::events::OutputState;
        let events = [
            make_record_state_event(true, &OutputState::Starting, None),
            make_record_state_event(false, &OutputState::Stopped, None),
            make_stream_event(true, &OutputState::Started),
            make_stream_event(false, &OutputState::Stopped),
            make_virtualcam_event(true, &OutputState::Started),
            make_virtualcam_event(false, &OutputState::Stopped),
        ];
        for ev in events {
            assert!(
                ev.payload.get("output_state").is_none(),
                "output_state should be dropped from {}",
                ev.kind,
            );
        }
    }

    #[test]
    fn studio_mode_events_drop_redundant_enabled_field() {
        for enabled in [true, false] {
            let mapped = map_obs_event(
                &obws::events::Event::StudioModeStateChanged { enabled },
                None,
                None,
            );
            assert!(mapped.is_some(), "studio mode event should map");
            let carries_enabled = mapped
                .as_ref()
                .is_some_and(|ev| ev.payload.get("enabled").is_some());
            assert!(!carries_enabled, "enabled should be dropped");
        }
    }

    #[test]
    fn every_emitted_kind_is_namespaced_under_obs() {
        use obws::events::OutputState;
        let events = [
            make_connection_connected(),
            make_connection_disconnected("connection_lost", None),
            make_connection_auth_failed("bad"),
            make_scene_changed_event(Some("A"), "B", None),
            make_record_event(true, None),
            make_record_state_event(true, &OutputState::Starting, None),
            make_stream_event(true, &OutputState::Started),
            make_virtualcam_event(true, &OutputState::Started),
            map_scene_item_visibility("Scene", "Src", true),
            map_scene_item_lock("Scene", "Src", true),
        ];
        for ev in events {
            assert!(
                ev.kind.starts_with("obs."),
                "kind is not namespaced under obs: {}",
                ev.kind,
            );
        }
    }
}
