use std::collections::HashMap;

use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;
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
use crate::source::SourceInfo;

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
        obws::events::Event::SceneItemCreated { scene, source, .. } => {
            let items = catalog.sources.entry(scene.name.clone()).or_default();
            if !items.iter().any(|s| s.name == source.name) {
                items.push(SourceInfo {
                    name: source.name.clone(),
                    visible: true,
                    locked: false,
                    audio_db: None,
                    kind: None,
                });
            }
        }
        obws::events::Event::SceneItemRemoved { scene, source, .. } => {
            if let Some(items) = catalog.sources.get_mut(&scene.name) {
                items.retain(|s| s.name != source.name);
            }
        }
        obws::events::Event::InputCreated {
            id,
            unversioned_kind,
            ..
        } => {
            if crate::catalog::is_audio_kind(Some(unversioned_kind))
                && !catalog.audio_inputs.iter().any(|n| n == &id.name)
            {
                catalog.audio_inputs.push(id.name.clone());
            }
        }
        obws::events::Event::InputRemoved { id } => {
            catalog.audio_inputs.retain(|n| n != &id.name);
            for items in catalog.sources.values_mut() {
                items.retain(|s| s.name != id.name);
            }
        }
        obws::events::Event::InputNameChanged {
            old_name, new_name, ..
        } => {
            for name in &mut catalog.audio_inputs {
                if name == old_name {
                    *name = new_name.clone();
                }
            }
            for items in catalog.sources.values_mut() {
                for item in items.iter_mut() {
                    if &item.name == old_name {
                        item.name = new_name.clone();
                    }
                }
            }
        }
        obws::events::Event::InputVolumeChanged { id, db, .. } => {
            for items in catalog.sources.values_mut() {
                for item in items.iter_mut() {
                    if item.name == id.name {
                        item.audio_db = Some(*db as f32);
                    }
                }
            }
        }
        obws::events::Event::StudioModeStateChanged { enabled: false } => {
            catalog.current_preview_scene = None;
        }
        _ => {}
    }
}

/// Only the `active` flag flips instantly here; duration/paused/dropped-frame figures are owned
/// by the 2s status poll (`apply_stream_status_update` / `apply_record_status_update`) since OBS
/// WebSocket v5 carries no duration field on these events.
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
                new_value: crate::health::stream_health_value(*active, snapshot.stream_duration),
            }]
        }
        obws::events::Event::RecordStateChanged { active, .. } => {
            if snapshot.record_active == *active {
                return vec![];
            }
            snapshot.record_active = *active;
            vec![HealthDelta {
                index: 1,
                new_value: crate::health::record_health_value(
                    *active,
                    snapshot.record_paused,
                    snapshot.record_duration,
                ),
            }]
        }
        _ => vec![],
    }
}

/// Only emits a delta when the rendered value actually changed. Dropped-frame figures come from
/// the stream-status poll (`apply_stream_status_update`), not from `GetStats`.
pub(crate) fn apply_stats_update(
    stats: &obws::responses::general::Stats,
    snapshot: &mut HealthSnapshot,
) -> Vec<HealthDelta> {
    let render_lag = stats.render_skipped_frames > 0 || stats.output_skipped_frames > 0;
    let unchanged = (snapshot.cpu_percent - stats.cpu_usage).abs() <= f64::EPSILON
        && (snapshot.fps - stats.active_fps).abs() <= f64::EPSILON
        && snapshot.render_lag == render_lag;
    if unchanged {
        return vec![];
    }
    snapshot.cpu_percent = stats.cpu_usage;
    snapshot.fps = stats.active_fps;
    snapshot.render_lag = render_lag;
    vec![HealthDelta {
        index: 2,
        new_value: crate::health::cpu_fps_value(
            snapshot.cpu_percent,
            snapshot.fps,
            snapshot.render_lag,
        ),
    }]
}

/// Feeds the Stream stat card (active + duration) and the Dropped stat card from one
/// `GetStreamStatus` poll.
pub(crate) fn apply_stream_status_update(
    status: &obws::responses::streaming::StreamStatus,
    snapshot: &mut HealthSnapshot,
) -> Vec<HealthDelta> {
    let duration = Some(status.duration.unsigned_abs());
    let dropped = u64::from(status.skipped_frames);
    let total = u64::from(status.total_frames);
    let mut deltas = Vec::new();

    if snapshot.stream_active != status.active || snapshot.stream_duration != duration {
        snapshot.stream_active = status.active;
        snapshot.stream_duration = duration;
        deltas.push(HealthDelta {
            index: 0,
            new_value: crate::health::stream_health_value(status.active, duration),
        });
    }

    if snapshot.dropped_frames != dropped || snapshot.total_frames != total {
        snapshot.dropped_frames = dropped;
        snapshot.total_frames = total;
        deltas.push(HealthDelta {
            index: 3,
            new_value: crate::health::dropped_value(dropped, total),
        });
    }

    deltas
}

/// Feeds the Recording stat card (active + paused + duration) from one `GetRecordStatus` poll.
pub(crate) fn apply_record_status_update(
    status: &obws::responses::recording::RecordStatus,
    snapshot: &mut HealthSnapshot,
) -> Vec<HealthDelta> {
    let duration = Some(status.duration.unsigned_abs());
    let unchanged = snapshot.record_active == status.active
        && snapshot.record_paused == status.paused
        && snapshot.record_duration == duration;
    if unchanged {
        return vec![];
    }
    snapshot.record_active = status.active;
    snapshot.record_paused = status.paused;
    snapshot.record_duration = duration;
    vec![HealthDelta {
        index: 1,
        new_value: crate::health::record_health_value(status.active, status.paused, duration),
    }]
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use std::time::Duration;

    use forge_platform_core::HealthValue;
    use obws::responses::general::Stats;
    use obws::responses::recording::RecordStatus;
    use obws::responses::streaming::StreamStatus;

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
    fn resolve_source_name_requires_both_the_scene_and_the_item_id_to_match() {
        let mut cache = HashMap::new();
        cache.insert(("Gameplay".to_owned(), "Game Capture".to_owned()), 42i64);
        for (scene, item_id) in [("BRB", 42u64), ("Gameplay", 99u64), ("BRB", 99u64)] {
            assert!(
                resolve_source_name(&cache, scene, item_id).is_none(),
                "resolved {scene}/{item_id} against a cache that holds Gameplay/42",
            );
        }
        assert!(resolve_source_name(&HashMap::new(), "Gameplay", 42u64).is_none());
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

    fn stream_status(active: bool, secs: i64, skipped: u32, total: u32) -> StreamStatus {
        StreamStatus {
            active,
            duration: time::Duration::seconds(secs),
            skipped_frames: skipped,
            total_frames: total,
            ..StreamStatus::default()
        }
    }

    fn record_status(active: bool, paused: bool, secs: i64) -> RecordStatus {
        RecordStatus {
            active,
            paused,
            duration: time::Duration::seconds(secs),
            ..RecordStatus::default()
        }
    }

    #[test]
    fn stream_status_poll_emits_a_stream_and_a_dropped_delta_on_the_first_reading() {
        let mut snapshot = HealthSnapshot::default();

        let deltas =
            apply_stream_status_update(&stream_status(true, 3_720, 5, 1_000), &mut snapshot);

        assert_eq!(
            deltas.iter().map(|d| d.index).collect::<Vec<_>>(),
            vec![0, 3],
        );
        assert!(snapshot.stream_active);
        assert_eq!(snapshot.stream_duration, Some(Duration::from_secs(3_720)));
        assert_eq!(snapshot.dropped_frames, 5);
        assert_eq!(snapshot.total_frames, 1_000);
    }

    #[test]
    fn stream_status_poll_repeating_the_same_reading_emits_nothing() {
        let mut snapshot = HealthSnapshot::default();
        let status = stream_status(true, 90, 0, 300);
        apply_stream_status_update(&status, &mut snapshot);

        assert!(apply_stream_status_update(&status, &mut snapshot).is_empty());
    }

    #[test]
    fn stream_status_poll_emits_only_the_stream_delta_when_frame_counts_hold_steady() {
        let mut snapshot = HealthSnapshot::default();
        apply_stream_status_update(&stream_status(true, 90, 2, 300), &mut snapshot);

        let deltas = apply_stream_status_update(&stream_status(true, 120, 2, 300), &mut snapshot);

        assert_eq!(deltas.iter().map(|d| d.index).collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn stream_status_poll_clears_the_duration_and_active_flag_when_the_stream_stops() {
        let mut snapshot = HealthSnapshot::default();
        apply_stream_status_update(&stream_status(true, 600, 0, 900), &mut snapshot);

        apply_stream_status_update(&stream_status(false, 0, 0, 900), &mut snapshot);

        assert!(!snapshot.stream_active);
        assert_eq!(snapshot.stream_duration, Some(Duration::ZERO));
    }

    #[test]
    fn record_status_poll_emits_one_recording_delta_per_changed_reading() {
        let mut snapshot = HealthSnapshot::default();

        let started = apply_record_status_update(&record_status(true, false, 30), &mut snapshot);
        assert_eq!(started.iter().map(|d| d.index).collect::<Vec<_>>(), vec![1]);

        let paused = apply_record_status_update(&record_status(true, true, 30), &mut snapshot);
        assert_eq!(paused.len(), 1);
        assert!(snapshot.record_paused);

        assert!(
            apply_record_status_update(&record_status(true, true, 30), &mut snapshot).is_empty(),
            "an unchanged reading must not emit a delta",
        );
    }

    #[test]
    fn record_status_poll_reports_a_paused_recording_as_still_active() {
        let mut snapshot = HealthSnapshot::default();

        let deltas = apply_record_status_update(&record_status(true, true, 45), &mut snapshot);

        let HealthValue::Status { label, active, .. } = &deltas[0].new_value else {
            panic!("expected a Status value");
        };
        assert_eq!(label, "Paused");
        assert!(active);
    }

    #[test]
    fn stats_poll_emits_a_cpu_delta_only_when_a_rendered_figure_changes() {
        let mut snapshot = HealthSnapshot::default();
        let stats = Stats {
            cpu_usage: 12.5,
            active_fps: 60.0,
            ..Stats::default()
        };

        assert_eq!(apply_stats_update(&stats, &mut snapshot).len(), 1);
        assert!(apply_stats_update(&stats, &mut snapshot).is_empty());

        let lagging = Stats {
            render_skipped_frames: 1,
            ..stats.clone()
        };
        let deltas = apply_stats_update(&lagging, &mut snapshot);
        assert_eq!(deltas.iter().map(|d| d.index).collect::<Vec<_>>(), vec![2]);
        assert!(snapshot.render_lag);
    }

    #[test]
    fn stream_state_event_flips_the_active_flag_but_leaves_the_polled_duration_alone() {
        let mut snapshot = HealthSnapshot::default();
        apply_stream_status_update(&stream_status(true, 600, 0, 900), &mut snapshot);

        let deltas = apply_health_update(
            &obws::events::Event::StreamStateChanged {
                active: false,
                state: obws::events::OutputState::Stopped,
            },
            &mut snapshot,
        );

        assert_eq!(deltas.iter().map(|d| d.index).collect::<Vec<_>>(), vec![0]);
        assert!(!snapshot.stream_active);
        assert_eq!(snapshot.stream_duration, Some(Duration::from_secs(600)));
    }

    #[test]
    fn a_state_event_matching_the_current_flag_emits_nothing() {
        let mut snapshot = HealthSnapshot::default();

        let deltas = apply_health_update(
            &obws::events::Event::RecordStateChanged {
                active: false,
                state: obws::events::OutputState::Stopped,
                path: None,
            },
            &mut snapshot,
        );

        assert!(deltas.is_empty());
    }

    fn scene_id(name: &str) -> obws::responses::scenes::SceneId {
        obws::responses::scenes::SceneId {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn source_id(name: &str) -> obws::responses::sources::SourceId {
        obws::responses::sources::SourceId {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn input_id(name: &str) -> obws::responses::inputs::InputId {
        obws::responses::inputs::InputId {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn source_info(name: &str, kind: Option<&str>) -> SourceInfo {
        SourceInfo {
            name: name.to_owned(),
            visible: true,
            locked: false,
            audio_db: None,
            kind: kind.map(str::to_owned),
        }
    }

    fn source_names(catalog: &ObsCatalog, scene: &str) -> Vec<String> {
        catalog
            .sources
            .get(scene)
            .map(|items| items.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default()
    }

    fn scene_item_created(scene: &str, source: &str) -> obws::events::Event {
        obws::events::Event::SceneItemCreated {
            scene: scene_id(scene),
            source: source_id(source),
            item_id: 7,
            index: 0,
        }
    }

    fn input_created(name: &str, unversioned_kind: &str) -> obws::events::Event {
        obws::events::Event::InputCreated {
            id: input_id(name),
            kind: format!("{unversioned_kind}_v2"),
            unversioned_kind: unversioned_kind.to_owned(),
            caps: Default::default(),
            settings: json!({}),
            default_settings: json!({}),
        }
    }

    // OBS replays the full scene-item roster on some transitions, so the same creation can be seen
    // twice for one item; the panel must not grow a duplicate row.
    #[test]
    fn a_repeated_scene_item_creation_adds_the_source_only_once() {
        let mut catalog = ObsCatalog::default();

        apply_catalog_update(&scene_item_created("Gameplay", "Webcam"), &mut catalog);
        apply_catalog_update(&scene_item_created("Gameplay", "Webcam"), &mut catalog);

        assert_eq!(source_names(&catalog, "Gameplay"), vec!["Webcam"]);
    }

    #[test]
    fn a_removed_scene_item_leaves_the_same_source_standing_in_other_scenes() {
        let mut catalog = ObsCatalog::default();
        catalog
            .sources
            .insert("Gameplay".to_owned(), vec![source_info("Webcam", None)]);
        catalog
            .sources
            .insert("BRB".to_owned(), vec![source_info("Webcam", None)]);

        apply_catalog_update(
            &obws::events::Event::SceneItemRemoved {
                scene: scene_id("Gameplay"),
                source: source_id("Webcam"),
                item_id: 7,
            },
            &mut catalog,
        );

        assert!(source_names(&catalog, "Gameplay").is_empty());
        assert_eq!(source_names(&catalog, "BRB"), vec!["Webcam"]);
    }

    // The audio panel lists capture inputs only; a new browser or text source must not appear there.
    #[test]
    fn only_a_capture_input_joins_the_audio_input_roster_when_created() {
        for (kind, expected) in [
            ("wasapi_input_capture", vec!["Mic"]),
            ("browser_source", Vec::<&str>::new()),
        ] {
            let mut catalog = ObsCatalog::default();
            apply_catalog_update(&input_created("Mic", kind), &mut catalog);
            assert_eq!(catalog.audio_inputs, expected, "kind {kind}");
        }
    }

    #[test]
    fn a_repeated_audio_input_creation_registers_the_input_only_once() {
        let mut catalog = ObsCatalog::default();

        apply_catalog_update(&input_created("Mic", "wasapi_input_capture"), &mut catalog);
        apply_catalog_update(&input_created("Mic", "wasapi_input_capture"), &mut catalog);

        assert_eq!(catalog.audio_inputs, vec!["Mic"]);
    }

    #[test]
    fn a_removed_input_is_purged_from_the_audio_roster_and_from_every_scene() {
        let mut catalog = ObsCatalog {
            audio_inputs: vec!["Mic".to_owned(), "Desktop Audio".to_owned()],
            ..Default::default()
        };
        catalog.sources.insert(
            "Gameplay".to_owned(),
            vec![
                source_info("Mic", Some("wasapi_input_capture")),
                source_info("Webcam", None),
            ],
        );
        catalog.sources.insert(
            "BRB".to_owned(),
            vec![source_info("Mic", Some("wasapi_input_capture"))],
        );

        apply_catalog_update(
            &obws::events::Event::InputRemoved {
                id: input_id("Mic"),
            },
            &mut catalog,
        );

        assert_eq!(catalog.audio_inputs, vec!["Desktop Audio"]);
        assert_eq!(source_names(&catalog, "Gameplay"), vec!["Webcam"]);
        assert!(source_names(&catalog, "BRB").is_empty());
    }

    #[test]
    fn a_renamed_input_is_followed_in_the_audio_roster_and_in_every_scene() {
        let mut catalog = ObsCatalog {
            audio_inputs: vec!["Mic".to_owned()],
            ..Default::default()
        };
        catalog.sources.insert(
            "Gameplay".to_owned(),
            vec![source_info("Mic", Some("wasapi_input_capture"))],
        );
        catalog.sources.insert(
            "BRB".to_owned(),
            vec![source_info("Mic", Some("wasapi_input_capture"))],
        );

        apply_catalog_update(
            &obws::events::Event::InputNameChanged {
                uuid: Default::default(),
                old_name: "Mic".to_owned(),
                new_name: "Studio Mic".to_owned(),
            },
            &mut catalog,
        );

        assert_eq!(catalog.audio_inputs, vec!["Studio Mic"]);
        assert_eq!(source_names(&catalog, "Gameplay"), vec!["Studio Mic"]);
        assert_eq!(source_names(&catalog, "BRB"), vec!["Studio Mic"]);
    }

    #[test]
    fn a_volume_change_writes_the_level_on_that_input_in_every_scene_it_appears_in() {
        let mut catalog = ObsCatalog::default();
        catalog.sources.insert(
            "Gameplay".to_owned(),
            vec![
                source_info("Mic", Some("wasapi_input_capture")),
                source_info("Webcam", None),
            ],
        );
        catalog.sources.insert(
            "BRB".to_owned(),
            vec![source_info("Mic", Some("wasapi_input_capture"))],
        );

        apply_catalog_update(
            &obws::events::Event::InputVolumeChanged {
                id: input_id("Mic"),
                mul: 0.25,
                db: -12.5,
            },
            &mut catalog,
        );

        let levels = |scene: &str, source: &str| {
            catalog.sources[scene]
                .iter()
                .find(|s| s.name == source)
                .and_then(|s| s.audio_db)
        };
        assert_eq!(levels("Gameplay", "Mic"), Some(-12.5));
        assert_eq!(levels("BRB", "Mic"), Some(-12.5));
        assert_eq!(levels("Gameplay", "Webcam"), None);
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
