use forge_events::{Event, EventSource};
use forge_platform_core::HealthDelta;
use serde_json::json;

use crate::catalog::ObsCatalog;
use crate::health::{
    CPU_FPS_METRIC, DROPPED_METRIC, FrameCounterSample, HealthSnapshot, RECORDING_METRIC,
    STREAM_METRIC,
};
use crate::item_ids::SceneItemIds;
use crate::source::SourceInfo;
use crate::vocabulary::event_kinds;
use crate::vocabulary::fields::{
    audio as audio_fields, collection as collection_fields, connection as connection_fields,
    filter as filter_fields, profile as profile_fields, recording as recording_fields,
    scene as scene_fields, source as source_fields, streaming as streaming_fields,
    transition as transition_fields, virtualcam as virtualcam_fields,
};

pub(crate) fn make_connection_connected() -> Event {
    Event::new(
        EventSource::Obs,
        event_kinds::CONNECTION_CONNECTED,
        json!({}),
    )
}

pub(crate) fn make_connection_disconnected(reason: &str, detail: Option<&str>) -> Event {
    Event::new(
        EventSource::Obs,
        event_kinds::CONNECTION_DISCONNECTED,
        json!({
            (connection_fields::REASON): reason,
            (connection_fields::DETAIL): detail,
        }),
    )
}

pub(crate) fn make_connection_auth_failed(message: &str) -> Event {
    Event::new(
        EventSource::Obs,
        event_kinds::CONNECTION_AUTH_FAILED,
        json!({ (connection_fields::ERROR_MESSAGE): message }),
    )
}

pub(crate) fn make_scene_changed_event(from_scene: Option<&str>, to_scene: &str) -> Event {
    let payload = json!({
        (scene_fields::FROM_SCENE): from_scene.unwrap_or(""),
        (scene_fields::TO_SCENE): to_scene,
    });
    Event::new(EventSource::Obs, event_kinds::SCENE_CHANGED, payload)
}

pub(crate) fn make_record_event(active: bool, path: Option<&str>) -> Event {
    let kind = if active {
        event_kinds::RECORDING_STARTED
    } else {
        event_kinds::RECORDING_STOPPED
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
        OutputState::Starting => event_kinds::RECORDING_STARTING,
        OutputState::Stopping => event_kinds::RECORDING_STOPPING,
        OutputState::Paused => event_kinds::RECORDING_PAUSED,
        OutputState::Resumed => event_kinds::RECORDING_RESUMED,
        _ => {
            if active {
                event_kinds::RECORDING_STARTED
            } else {
                event_kinds::RECORDING_STOPPED
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
        OutputState::Starting => event_kinds::VIRTUALCAM_STARTING,
        OutputState::Started => event_kinds::VIRTUALCAM_STARTED,
        OutputState::Stopping => event_kinds::VIRTUALCAM_STOPPING,
        OutputState::Stopped => event_kinds::VIRTUALCAM_STOPPED,
        _ => {
            if active {
                event_kinds::VIRTUALCAM_STARTED
            } else {
                event_kinds::VIRTUALCAM_STOPPED
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
        OutputState::Starting => event_kinds::STREAMING_STARTING,
        OutputState::Started => event_kinds::STREAMING_STARTED,
        OutputState::Stopping => event_kinds::STREAMING_STOPPING,
        OutputState::Stopped => event_kinds::STREAMING_STOPPED,
        OutputState::Reconnecting => event_kinds::STREAMING_RECONNECTING,
        OutputState::Reconnected => event_kinds::STREAMING_RECONNECTED,
        _ => {
            if active {
                event_kinds::STREAMING_STARTED
            } else {
                event_kinds::STREAMING_STOPPED
            }
        }
    };
    Event::new(
        EventSource::Obs,
        kind,
        json!({ (streaming_fields::IS_ACTIVE): active }),
    )
}

pub(crate) fn map_obs_event(ev: &obws::events::Event, from_scene: Option<&str>) -> Option<Event> {
    match ev {
        obws::events::Event::CurrentProgramSceneChanged { id } => {
            Some(make_scene_changed_event(from_scene, &id.name))
        }
        obws::events::Event::CurrentPreviewSceneChanged { id } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SCENE_PREVIEW_CHANGED,
            json!({
                (scene_fields::SCENE_NAME_OLD): from_scene.unwrap_or(""),
                (scene_fields::SCENE_NAME_NEW): id.name,
            }),
        )),
        obws::events::Event::SceneListChanged { scenes } => {
            let names: Vec<&str> = scenes.iter().map(|s| s.name.as_str()).collect();
            Some(Event::new(
                EventSource::Obs,
                event_kinds::SCENE_LIST_CHANGED,
                json!({ (scene_fields::ALL_NAMES): names }),
            ))
        }
        obws::events::Event::SceneCreated { id, .. } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SCENE_CREATED,
            json!({ (scene_fields::SCENE_NAME): id.name }),
        )),
        obws::events::Event::SceneRemoved { id, .. } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SCENE_REMOVED,
            json!({ (scene_fields::SCENE_NAME): id.name }),
        )),
        obws::events::Event::SceneNameChanged {
            old_name, new_name, ..
        } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SCENE_RENAMED,
            json!({
                (scene_fields::SCENE_NAME_OLD): old_name,
                (scene_fields::SCENE_NAME_NEW): new_name,
            }),
        )),
        obws::events::Event::CurrentProfileChanged { name } => Some(Event::new(
            EventSource::Obs,
            event_kinds::PROFILE_CURRENT_CHANGED,
            json!({ (profile_fields::PROFILE_NAME): name }),
        )),
        obws::events::Event::ProfileListChanged { profiles } => Some(Event::new(
            EventSource::Obs,
            event_kinds::PROFILE_LIST_CHANGED,
            json!({ (profile_fields::ALL_NAMES): profiles }),
        )),
        obws::events::Event::SceneCollectionListChanged { collections } => Some(Event::new(
            EventSource::Obs,
            event_kinds::COLLECTION_LIST_CHANGED,
            json!({ (collection_fields::ALL_NAMES): collections }),
        )),
        obws::events::Event::CurrentSceneCollectionChanging { name } => Some(Event::new(
            EventSource::Obs,
            event_kinds::COLLECTION_CHANGING,
            json!({ (collection_fields::COLLECTION_NAME): name }),
        )),
        obws::events::Event::CurrentSceneCollectionChanged { name } => Some(Event::new(
            EventSource::Obs,
            event_kinds::COLLECTION_CHANGED,
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
            event_kinds::RECORDING_FILE_CHANGED,
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
                event_kinds::STUDIO_ENABLED
            } else {
                event_kinds::STUDIO_DISABLED
            };
            Some(Event::new(EventSource::Obs, kind, json!({})))
        }
        obws::events::Event::SceneTransitionStarted { id } => Some(Event::new(
            EventSource::Obs,
            event_kinds::TRANSITION_STARTED,
            json!({ (transition_fields::TRANSITION_NAME): id.name }),
        )),
        obws::events::Event::SceneTransitionEnded { id } => Some(Event::new(
            EventSource::Obs,
            event_kinds::TRANSITION_ENDED,
            json!({ (transition_fields::TRANSITION_NAME): id.name }),
        )),
        obws::events::Event::SceneTransitionVideoEnded { id } => Some(Event::new(
            EventSource::Obs,
            event_kinds::TRANSITION_VIDEO_ENDED,
            json!({ (transition_fields::TRANSITION_NAME): id.name }),
        )),
        obws::events::Event::InputMuteStateChanged { id, muted } => Some(Event::new(
            EventSource::Obs,
            event_kinds::AUDIO_SOURCE_MUTE_CHANGED,
            json!({ (audio_fields::SOURCE_NAME): id.name, (audio_fields::IS_MUTED): muted }),
        )),
        obws::events::Event::InputVolumeChanged { id, mul, db } => Some(Event::new(
            EventSource::Obs,
            event_kinds::AUDIO_SOURCE_VOLUME_CHANGED,
            json!({
                (audio_fields::SOURCE_NAME): id.name,
                (audio_fields::VOLUME_DB): db,
                (audio_fields::VOLUME_MULTIPLIER): mul,
            }),
        )),
        obws::events::Event::InputAudioBalanceChanged { id, audio_balance } => Some(Event::new(
            EventSource::Obs,
            event_kinds::AUDIO_SOURCE_BALANCE_CHANGED,
            json!({ (audio_fields::SOURCE_NAME): id.name, (audio_fields::BALANCE): audio_balance }),
        )),
        obws::events::Event::InputAudioSyncOffsetChanged { id, offset } => Some(Event::new(
            EventSource::Obs,
            event_kinds::AUDIO_SOURCE_SYNC_OFFSET_CHANGED,
            json!({
                (audio_fields::SOURCE_NAME): id.name,
                (audio_fields::SYNC_OFFSET_MS): offset.whole_milliseconds(),
            }),
        )),
        obws::events::Event::SceneItemCreated {
            scene,
            source,
            item_id,
            ..
        } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SOURCE_SCENE_ITEM_CREATED,
            json!({
                (source_fields::SCENE_NAME): scene.name,
                (source_fields::SOURCE_NAME): source.name,
                (source_fields::ITEM_ID): item_id,
            }),
        )),
        obws::events::Event::SceneItemRemoved {
            scene,
            source,
            item_id,
        } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SOURCE_SCENE_ITEM_REMOVED,
            json!({
                (source_fields::SCENE_NAME): scene.name,
                (source_fields::SOURCE_NAME): source.name,
                (source_fields::ITEM_ID): item_id,
            }),
        )),
        obws::events::Event::InputCreated {
            id,
            unversioned_kind,
            ..
        } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SOURCE_INPUT_CREATED,
            json!({
                (source_fields::SOURCE_NAME): id.name,
                (source_fields::SOURCE_KIND): unversioned_kind,
            }),
        )),
        obws::events::Event::InputRemoved { id } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SOURCE_INPUT_REMOVED,
            json!({ (source_fields::SOURCE_NAME): id.name }),
        )),
        obws::events::Event::InputNameChanged {
            old_name, new_name, ..
        } => Some(Event::new(
            EventSource::Obs,
            event_kinds::SOURCE_INPUT_RENAMED,
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
            event_kinds::FILTER_CREATED,
            json!({
                (filter_fields::SOURCE_NAME): source,
                (filter_fields::FILTER_NAME): filter,
                (filter_fields::FILTER_KIND): kind,
            }),
        )),
        obws::events::Event::SourceFilterRemoved { source, filter } => Some(Event::new(
            EventSource::Obs,
            event_kinds::FILTER_REMOVED,
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
            event_kinds::FILTER_ENABLED_CHANGED,
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
        event_kinds::SOURCE_VISIBILITY_CHANGED,
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
        event_kinds::SOURCE_LOCK_CHANGED,
        json!({
            (source_fields::SCENE_NAME): scene,
            (source_fields::SOURCE_NAME): source,
            (source_fields::IS_LOCKED): locked,
        }),
    )
}

pub(crate) fn apply_item_id_update(ev: &obws::events::Event, ids: &mut SceneItemIds) {
    match ev {
        obws::events::Event::SceneItemCreated {
            scene,
            source,
            item_id,
            ..
        } => {
            if let Ok(item_id) = i64::try_from(*item_id) {
                ids.insert(&scene.name, item_id, &source.name);
            }
        }
        obws::events::Event::SceneItemRemoved { scene, item_id, .. } => {
            if let Ok(item_id) = i64::try_from(*item_id) {
                ids.remove_item(&scene.name, item_id);
            }
        }
        obws::events::Event::SceneRemoved { id, .. } => ids.remove_scene(&id.name),
        obws::events::Event::SceneNameChanged {
            old_name, new_name, ..
        } => ids.rename_scene(old_name, new_name),
        obws::events::Event::InputRemoved { id } => ids.remove_source(&id.name),
        obws::events::Event::InputNameChanged {
            old_name, new_name, ..
        } => ids.rename_source(old_name, new_name),
        obws::events::Event::CurrentSceneCollectionChanging { .. }
        | obws::events::Event::CurrentSceneCollectionChanged { .. } => ids.clear(),
        _ => {}
    }
}

pub(crate) fn apply_catalog_update(ev: &obws::events::Event, catalog: &mut ObsCatalog) -> bool {
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
        obws::events::Event::SceneCreated { id, .. } => {
            if !catalog.scenes.contains(&id.name) {
                catalog.scenes.push(id.name.clone());
            }
        }
        obws::events::Event::SceneRemoved { id, .. } => {
            catalog.scenes.retain(|s| s != &id.name);
            catalog.sources.remove(id.name.as_str());
            if catalog.current_scene.as_deref() == Some(id.name.as_str()) {
                catalog.current_scene = None;
            }
            if catalog.current_preview_scene.as_deref() == Some(id.name.as_str()) {
                catalog.current_preview_scene = None;
            }
        }
        obws::events::Event::SceneNameChanged {
            old_name, new_name, ..
        } => {
            for scene in &mut catalog.scenes {
                if scene == old_name {
                    scene.clone_from(new_name);
                }
            }
            if catalog.current_scene.as_deref() == Some(old_name.as_str()) {
                catalog.current_scene = Some(new_name.clone());
            }
            if catalog.current_preview_scene.as_deref() == Some(old_name.as_str()) {
                catalog.current_preview_scene = Some(new_name.clone());
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
                    name.clone_from(new_name);
                }
            }
            for items in catalog.sources.values_mut() {
                for item in items.iter_mut() {
                    if &item.name == old_name {
                        item.name.clone_from(new_name);
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
        obws::events::Event::CurrentSceneCollectionChanging { .. }
        | obws::events::Event::CurrentSceneCollectionChanged { .. } => {}
        _ => return false,
    }
    catalog.revision = catalog.revision.wrapping_add(1);
    true
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
            vec![crate::health::delta(STREAM_METRIC, snapshot)]
        }
        obws::events::Event::RecordStateChanged { active, state, .. } => {
            let paused = *state == obws::events::OutputState::Paused;
            if snapshot.record_active == *active && snapshot.record_paused == paused {
                return vec![];
            }
            snapshot.record_active = *active;
            snapshot.record_paused = paused;
            vec![crate::health::delta(RECORDING_METRIC, snapshot)]
        }
        _ => vec![],
    }
}

const OBS_FRAME_LAG_WARNING_PCT: f64 = 1.0;
const FRAME_LAG_PCT_BASE: f64 = 100.0;

fn frame_lag_detected(
    prev_sample: &mut Option<FrameCounterSample>,
    current: FrameCounterSample,
) -> bool {
    let lagging = match *prev_sample {
        Some(prev) if current.total >= prev.total && current.skipped >= prev.skipped => {
            let total_delta = current.total - prev.total;
            let skipped_delta = current.skipped - prev.skipped;
            total_delta > 0
                && (f64::from(skipped_delta) / f64::from(total_delta)) * FRAME_LAG_PCT_BASE
                    > OBS_FRAME_LAG_WARNING_PCT
        }
        _ => false,
    };
    *prev_sample = Some(current);
    lagging
}

pub(crate) fn apply_stats_update(
    stats: &obws::responses::general::Stats,
    snapshot: &mut HealthSnapshot,
) -> Vec<HealthDelta> {
    let render_lagging = frame_lag_detected(
        &mut snapshot.render_frame_sample,
        FrameCounterSample {
            skipped: stats.render_skipped_frames,
            total: stats.render_total_frames,
        },
    );
    let output_lagging = frame_lag_detected(
        &mut snapshot.output_frame_sample,
        FrameCounterSample {
            skipped: stats.output_skipped_frames,
            total: stats.output_total_frames,
        },
    );
    let render_lag = render_lagging || output_lagging;

    let unchanged = (snapshot.cpu_percent - stats.cpu_usage).abs() <= f64::EPSILON
        && (snapshot.fps - stats.active_fps).abs() <= f64::EPSILON
        && snapshot.render_lag == render_lag;
    if unchanged {
        return vec![];
    }
    snapshot.cpu_percent = stats.cpu_usage;
    snapshot.fps = stats.active_fps;
    snapshot.render_lag = render_lag;
    vec![crate::health::delta(CPU_FPS_METRIC, snapshot)]
}

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
        deltas.push(crate::health::delta(STREAM_METRIC, snapshot));
    }

    if snapshot.dropped_frames != dropped || snapshot.total_frames != total {
        snapshot.dropped_frames = dropped;
        snapshot.total_frames = total;
        deltas.push(crate::health::delta(DROPPED_METRIC, snapshot));
    }

    deltas
}

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
    vec![crate::health::delta(RECORDING_METRIC, snapshot)]
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_platform_core::{BuiltinHealth, HealthMetricId, HealthText, HealthValue};
    use obws::responses::general::Stats;
    use obws::responses::recording::RecordStatus;
    use obws::responses::streaming::StreamStatus;

    use super::*;

    #[test]
    fn make_scene_changed_event_emits_from_and_to_fields() {
        let ev = make_scene_changed_event(Some("Menu"), "Gameplay");
        assert_eq!(ev.source, EventSource::Obs);
        assert_eq!(ev.kind, "obs.scene.changed");
        assert_eq!(ev.payload["from_scene"], "Menu");
        assert_eq!(ev.payload["to_scene"], "Gameplay");
        assert!(ev.caused_by.is_none());
    }

    #[test]
    fn make_scene_changed_event_unknown_from_scene_uses_empty_string() {
        let ev = make_scene_changed_event(None, "Gameplay");
        assert_eq!(ev.payload["from_scene"], "");
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
            crate::vocabulary::fields::connection::reason::CONNECTION_LOST,
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
            deltas.iter().map(|d| d.metric.clone()).collect::<Vec<_>>(),
            vec![STREAM_METRIC, DROPPED_METRIC],
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

        assert_eq!(
            deltas.iter().map(|d| d.metric.clone()).collect::<Vec<_>>(),
            vec![STREAM_METRIC]
        );
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
        assert_eq!(
            started.iter().map(|d| d.metric.clone()).collect::<Vec<_>>(),
            vec![RECORDING_METRIC]
        );

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
        assert_eq!(
            label,
            &HealthText::from(crate::vocabulary::messages::health_paused())
        );
        assert!(active);
    }

    fn record_state_changed(active: bool, state: obws::events::OutputState) -> obws::events::Event {
        obws::events::Event::RecordStateChanged {
            active,
            state,
            path: None,
        }
    }

    fn recording_label(deltas: &[HealthDelta]) -> &HealthText {
        match deltas {
            [
                HealthDelta {
                    metric,
                    new_value: HealthValue::Status { label, .. },
                },
            ] if *metric == RECORDING_METRIC => label,
            other => panic!("expected one recording delta, got {other:?}"),
        }
    }

    #[test]
    fn pausing_and_resuming_a_recording_each_repaint_the_tile_without_a_status_poll() {
        use crate::vocabulary::messages::{health_active, health_paused};
        use obws::events::OutputState;

        let mut snapshot = HealthSnapshot {
            record_active: true,
            ..HealthSnapshot::default()
        };

        let paused = apply_health_update(
            &record_state_changed(true, OutputState::Paused),
            &mut snapshot,
        );
        assert_eq!(recording_label(&paused), &health_paused().into());
        assert!(snapshot.record_paused);

        let resumed = apply_health_update(
            &record_state_changed(true, OutputState::Resumed),
            &mut snapshot,
        );
        assert_eq!(recording_label(&resumed), &health_active().into());
        assert!(!snapshot.record_paused);
    }

    #[test]
    fn stopping_a_paused_recording_clears_the_paused_flag() {
        use crate::vocabulary::messages::health_off;
        use obws::events::OutputState;

        let mut snapshot = HealthSnapshot {
            record_active: true,
            record_paused: true,
            ..HealthSnapshot::default()
        };

        let stopped = apply_health_update(
            &record_state_changed(false, OutputState::Stopped),
            &mut snapshot,
        );

        assert_eq!(recording_label(&stopped), &health_off().into());
        assert!(!snapshot.record_paused);
    }

    #[test]
    fn a_repeated_record_state_emits_no_delta() {
        use obws::events::OutputState;

        let mut snapshot = HealthSnapshot {
            record_active: true,
            record_paused: true,
            ..HealthSnapshot::default()
        };

        assert!(
            apply_health_update(
                &record_state_changed(true, OutputState::Paused),
                &mut snapshot
            )
            .is_empty()
        );
    }

    type FrameCounters = (u32, u32);

    const IDLE_FRAMES: FrameCounters = (0, 0);
    const SMOOTH: bool = false;
    const LAGGING: bool = true;

    fn stats(cpu: f64, fps: f64, render: FrameCounters, output: FrameCounters) -> Stats {
        Stats {
            cpu_usage: cpu,
            active_fps: fps,
            render_skipped_frames: render.0,
            render_total_frames: render.1,
            output_skipped_frames: output.0,
            output_total_frames: output.1,
            ..Stats::default()
        }
    }

    #[test]
    fn stats_poll_emits_a_cpu_delta_only_when_a_rendered_figure_changes() {
        let mut snapshot = HealthSnapshot::default();

        for (reading, expected, case) in [
            (
                stats(12.5, 60.0, IDLE_FRAMES, IDLE_FRAMES),
                true,
                "first reading",
            ),
            (
                stats(12.5, 60.0, IDLE_FRAMES, IDLE_FRAMES),
                false,
                "an identical reading",
            ),
            (
                stats(13.0, 60.0, IDLE_FRAMES, IDLE_FRAMES),
                true,
                "cpu moved",
            ),
            (
                stats(13.0, 59.0, IDLE_FRAMES, IDLE_FRAMES),
                true,
                "fps moved",
            ),
            (
                stats(13.0, 59.0, (60, 600), IDLE_FRAMES),
                true,
                "the lag verdict flipped",
            ),
            (
                stats(13.0, 59.0, (120, 1_200), IDLE_FRAMES),
                false,
                "still lagging on unchanged cpu and fps",
            ),
        ] {
            let indexes: Vec<HealthMetricId> = apply_stats_update(&reading, &mut snapshot)
                .iter()
                .map(|d| d.metric.clone())
                .collect();
            let expected = if expected {
                vec![CPU_FPS_METRIC]
            } else {
                vec![]
            };
            assert_eq!(indexes, expected, "{case}");
        }
    }

    #[test]
    fn stats_poll_judges_frame_lag_on_the_share_of_frames_skipped_since_the_previous_poll() {
        for (case, polls) in [
            (
                "a cumulative skip count carries no verdict on the first poll",
                vec![((500, 10_000), IDLE_FRAMES, SMOOTH)],
            ),
            (
                "a stray skipped frame in a full interval stays smooth",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((1, 120), IDLE_FRAMES, SMOOTH),
                ],
            ),
            (
                "sustained skipping trips the warning",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((30, 600), IDLE_FRAMES, LAGGING),
                ],
            ),
            (
                "the threshold share itself is still smooth, one frame past it is not",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((1, 100), IDLE_FRAMES, SMOOTH),
                    ((3, 200), IDLE_FRAMES, LAGGING),
                ],
            ),
            (
                "a clean interval clears the warning without an OBS restart",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((30, 600), IDLE_FRAMES, LAGGING),
                    ((30, 1_200), IDLE_FRAMES, SMOOTH),
                ],
            ),
            (
                "a skip counter that went down rebaselines, and the next poll judges against it",
                vec![
                    ((100, 5_000), IDLE_FRAMES, SMOOTH),
                    ((30, 600), IDLE_FRAMES, SMOOTH),
                    ((60, 1_200), IDLE_FRAMES, LAGGING),
                ],
            ),
            (
                "a frame counter that went down rebaselines, and the next poll judges against it",
                vec![
                    ((0, 5_000), IDLE_FRAMES, SMOOTH),
                    ((0, 600), IDLE_FRAMES, SMOOTH),
                    ((30, 1_200), IDLE_FRAMES, LAGGING),
                ],
            ),
            (
                "an interval that produced no frames cannot be lagging",
                vec![
                    ((10, 500), IDLE_FRAMES, SMOOTH),
                    ((10, 500), IDLE_FRAMES, SMOOTH),
                    ((15, 500), IDLE_FRAMES, SMOOTH),
                ],
            ),
            (
                "render skipping alone trips the warning",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((30, 600), (0, 600), LAGGING),
                ],
            ),
            (
                "output skipping alone trips the warning",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((0, 600), (30, 600), LAGGING),
                ],
            ),
            (
                "a poll that emits no delta still advances the baseline",
                vec![
                    (IDLE_FRAMES, IDLE_FRAMES, SMOOTH),
                    ((0, 10_000), IDLE_FRAMES, SMOOTH),
                    ((60, 10_600), IDLE_FRAMES, LAGGING),
                ],
            ),
        ] {
            let mut snapshot = HealthSnapshot::default();
            for (poll, (render, output, expected)) in polls.into_iter().enumerate() {
                apply_stats_update(&stats(12.5, 60.0, render, output), &mut snapshot);
                assert_eq!(snapshot.render_lag, expected, "{case}: poll {poll}");
            }
        }
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

        assert_eq!(
            deltas.iter().map(|d| d.metric.clone()).collect::<Vec<_>>(),
            vec![STREAM_METRIC]
        );
        assert!(!snapshot.stream_active);
        assert_eq!(snapshot.stream_duration, Some(Duration::from_secs(600)));
    }

    #[test]
    fn every_health_delta_producer_targets_a_declared_metric() {
        let declared: Vec<HealthMetricId> =
            crate::client::ObsClient::new_for_test("localhost:4455".to_owned())
                .metrics()
                .into_iter()
                .map(|metric| metric.id)
                .collect();
        let mut snapshot = HealthSnapshot::default();

        let mut deltas =
            apply_stream_status_update(&stream_status(true, 60, 3, 600), &mut snapshot);
        deltas.extend(apply_record_status_update(
            &record_status(true, false, 10),
            &mut snapshot,
        ));
        deltas.extend(apply_stats_update(
            &stats(10.0, 60.0, IDLE_FRAMES, IDLE_FRAMES),
            &mut snapshot,
        ));
        deltas.extend(apply_health_update(
            &obws::events::Event::RecordStateChanged {
                active: false,
                state: obws::events::OutputState::Stopped,
                path: None,
            },
            &mut snapshot,
        ));

        assert!(!deltas.is_empty());
        for delta in &deltas {
            assert!(
                declared.contains(&delta.metric),
                "undeclared {}",
                delta.metric
            );
        }
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

    #[test]
    fn a_repeated_scene_item_creation_adds_the_source_only_once() {
        let mut catalog = ObsCatalog::default();

        apply_catalog_update(&scene_item_created("Gameplay", "Webcam"), &mut catalog);
        apply_catalog_update(&scene_item_created("Gameplay", "Webcam"), &mut catalog);

        assert_eq!(source_names(&catalog, "Gameplay"), vec!["Webcam"]);
    }

    #[test]
    fn an_applied_event_moves_the_catalog_revision_and_an_unrelated_one_does_not() {
        use obws::events::Event;
        let collection = || "Collab".to_owned();
        for (event, applied) in [
            (
                Event::SceneCreated {
                    id: scene_id("Intro"),
                    is_group: false,
                },
                true,
            ),
            (
                Event::SceneRemoved {
                    id: scene_id("Absent"),
                    is_group: false,
                },
                true,
            ),
            (
                Event::CurrentSceneCollectionChanging { name: collection() },
                true,
            ),
            (
                Event::CurrentSceneCollectionChanged { name: collection() },
                true,
            ),
            (Event::StudioModeStateChanged { enabled: false }, true),
            (Event::StudioModeStateChanged { enabled: true }, false),
            (
                Event::SceneItemLockStateChanged {
                    scene: scene_id("Gameplay"),
                    item_id: 1,
                    locked: true,
                },
                false,
            ),
            (Event::CurrentProfileChanged { name: collection() }, false),
        ] {
            let mut catalog = ObsCatalog::default();

            let reported = apply_catalog_update(&event, &mut catalog);

            assert_eq!(reported, applied, "{event:?}");
            assert_eq!(catalog.revision, u64::from(applied), "{event:?}");
        }
    }

    #[test]
    fn the_catalog_revision_wraps_instead_of_overflowing() {
        let mut catalog = ObsCatalog {
            revision: u64::MAX,
            ..ObsCatalog::default()
        };

        apply_catalog_update(
            &obws::events::Event::SceneCreated {
                id: scene_id("Intro"),
                is_group: false,
            },
            &mut catalog,
        );

        assert_eq!(catalog.revision, 0);
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
    fn only_leaving_studio_mode_clears_the_preview_scene() {
        for (enabled, expected) in [(false, None), (true, Some("BRB"))] {
            let mut catalog = ObsCatalog {
                current_preview_scene: Some("BRB".to_owned()),
                ..ObsCatalog::default()
            };

            apply_catalog_update(
                &obws::events::Event::StudioModeStateChanged { enabled },
                &mut catalog,
            );

            assert_eq!(
                catalog.current_preview_scene.as_deref(),
                expected,
                "studio mode enabled={enabled}",
            );
        }
    }

    #[test]
    fn a_scene_item_added_or_removed_is_announced_with_its_scene_source_and_item_id() {
        for (ev, expected_kind) in [
            (
                obws::events::Event::SceneItemCreated {
                    scene: scene_id("Gameplay"),
                    source: source_id("Webcam"),
                    item_id: 42,
                    index: 3,
                },
                "obs.source.scene_item_created",
            ),
            (
                obws::events::Event::SceneItemRemoved {
                    scene: scene_id("Gameplay"),
                    source: source_id("Webcam"),
                    item_id: 42,
                },
                "obs.source.scene_item_removed",
            ),
        ] {
            let Some(mapped) = map_obs_event(&ev, None) else {
                panic!("{expected_kind} was not mapped");
            };

            assert_eq!(mapped.source, EventSource::Obs, "{expected_kind}");
            assert_eq!(mapped.kind, expected_kind);
            assert_eq!(
                mapped.payload,
                json!({ "scene_name": "Gameplay", "source_name": "Webcam", "item_id": 42 }),
                "{expected_kind}"
            );
        }
    }

    #[test]
    fn every_emitted_kind_is_namespaced_under_obs() {
        use obws::events::OutputState;
        let events = [
            make_connection_connected(),
            make_connection_disconnected("connection_lost", None),
            make_connection_auth_failed("bad"),
            make_scene_changed_event(Some("A"), "B"),
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
