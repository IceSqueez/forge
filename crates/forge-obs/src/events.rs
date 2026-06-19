use std::collections::HashMap;

use forge_events::{Event, EventSource};
use forge_platform_core::{HealthDelta, HealthValue};
use forge_types::EventId;
use serde_json::json;

use crate::catalog::ObsCatalog;
use crate::health::HealthSnapshot;

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
        obws::events::Event::RecordStateChanged { active, path, .. } => {
            Some(make_record_event(*active, path.as_deref()))
        }
        obws::events::Event::StreamStateChanged { active, state } => {
            Some(make_stream_event(*active, state))
        }
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
                new_value: HealthValue::Status {
                    label: if *active {
                        "Live".to_owned()
                    } else {
                        "Offline".to_owned()
                    },
                    active: *active,
                    detail: None,
                },
            }]
        }
        obws::events::Event::RecordStateChanged { active, .. } => {
            if snapshot.record_active == *active {
                return vec![];
            }
            snapshot.record_active = *active;
            vec![HealthDelta {
                index: 1,
                new_value: HealthValue::Status {
                    label: if *active {
                        "Active".to_owned()
                    } else {
                        "Off".to_owned()
                    },
                    active: *active,
                    detail: None,
                },
            }]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // obws::events::Event is #[non_exhaustive] — variants cannot be constructed outside
    // the defining crate, so map_obs_event / apply_catalog_update / apply_health_update
    // can't be unit-tested. The payload-builder helpers make_scene_changed_event and
    // make_record_event are tested here directly.
    //
    // COVERAGE GAP: make_stream_event takes &obws::events::OutputState, which is ALSO
    // #[non_exhaustive] (obws 0.15.0 src/events.rs). Its variants cannot be constructed
    // in this crate, so the kind/state-string mapping (Starting..Resumed + the active-based
    // fallback arm) has no constructible fixture and is left untested per the no-real-OBS
    // rule. The downstream contract IS covered: the streaming.* trigger descriptors assert
    // kind-discrimination and the build_stream_arg_stack output_state/is_active extraction.

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
