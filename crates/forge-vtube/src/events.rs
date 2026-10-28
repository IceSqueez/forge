use forge_events::{Event, EventPublisher, EventSource};
use serde_json::json;

use crate::payload_fields::{
    hotkey as hotkey_fields, item as item_fields, model as model_fields,
    tracking as tracking_fields,
};
use crate::vts::event::{ItemChange, VtsEvent};

pub(crate) fn dispatch_vts_event(event: &VtsEvent, publisher: &dyn EventPublisher) {
    match event {
        VtsEvent::ModelLoaded {
            loaded,
            model_id,
            model_name,
        } => {
            let kind = if *loaded {
                "vtube.model.loaded"
            } else {
                "vtube.model.unloaded"
            };
            publisher.publish(Event::new(
                EventSource::VTube,
                kind,
                json!({ (model_fields::MODEL_ID): model_id, (model_fields::MODEL_NAME): model_name }),
            ));
        }
        VtsEvent::ModelConfigChanged {
            model_id,
            model_name,
        } => {
            publisher.publish(Event::new(
                EventSource::VTube,
                "vtube.model.config_changed",
                json!({ (model_fields::MODEL_ID): model_id, (model_fields::MODEL_NAME): model_name }),
            ));
        }
        VtsEvent::HotkeyTriggered {
            hotkey_id,
            hotkey_name,
        } => {
            publisher.publish(Event::new(
                EventSource::VTube,
                "vtube.hotkey.triggered",
                json!({ (hotkey_fields::HOTKEY_ID): hotkey_id, (hotkey_fields::HOTKEY_NAME): hotkey_name }),
            ));
        }
        VtsEvent::TrackingStatusChanged {
            face_found,
            left_hand_found,
            right_hand_found,
        } => {
            let kind = if *face_found {
                "vtube.tracking.face_found"
            } else {
                "vtube.tracking.face_lost"
            };
            publisher.publish(Event::new(
                EventSource::VTube,
                kind,
                json!({
                    (tracking_fields::IS_LEFT_HAND_FOUND): left_hand_found,
                    (tracking_fields::IS_RIGHT_HAND_FOUND): right_hand_found,
                }),
            ));
        }
        VtsEvent::ItemChanged {
            change,
            item_instance_id,
            item_file_name,
        } => {
            let kind = match change {
                ItemChange::Added => "vtube.item.added",
                ItemChange::Removed => "vtube.item.removed",
            };
            publisher.publish(Event::new(
                EventSource::VTube,
                kind,
                json!({
                    (item_fields::ITEM_INSTANCE_ID): item_instance_id,
                    (item_fields::ITEM_FILE_NAME): item_file_name,
                }),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_events::EventPublisher;

    use super::*;
    use crate::client::tests::MockPublisher;

    fn make_envelope(message_type: &str, data: serde_json::Value) -> RawEnvelope {
        RawEnvelope {
            message_type: message_type.to_owned(),
            data,
        }
    }

    #[test]
    fn model_loaded_event_emits_model_loaded() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ModelLoadedEvent",
            serde_json::json!({
                "modelLoaded": true,
                "modelID": "model-abc",
                "modelName": "MyAvatar"
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.model.loaded")
            .unwrap();
        assert_eq!(ev.source, EventSource::VTube);
        assert_eq!(ev.payload["model_id"], "model-abc");
        assert_eq!(ev.payload["model_name"], "MyAvatar");
    }

    #[test]
    fn model_unloaded_event_emits_model_unloaded() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ModelLoadedEvent",
            serde_json::json!({
                "modelLoaded": false,
                "modelID": "model-xyz",
                "modelName": "OldAvatar"
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.model.unloaded")
            .unwrap();
        assert_eq!(ev.source, EventSource::VTube);
        assert_eq!(ev.payload["model_id"], "model-xyz");
        assert_eq!(ev.payload["model_name"], "OldAvatar");
    }

    #[test]
    fn hotkey_triggered_event_emits_hotkey_triggered() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "HotkeyTriggeredEvent",
            serde_json::json!({
                "hotkeyID": "hk-001",
                "hotkeyName": "BlushExpression"
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.hotkey.triggered")
            .unwrap();
        assert_eq!(ev.source, EventSource::VTube);
        assert_eq!(ev.payload["hotkey_id"], "hk-001");
        assert_eq!(ev.payload["hotkey_name"], "BlushExpression");
    }

    #[test]
    fn unknown_event_type_produces_no_bus_event() {
        let publisher = MockPublisher::new();
        let env = make_envelope("FaceFoundEvent", serde_json::json!({ "found": true }));

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "FaceFoundEvent should not produce a bus event"
        );
    }

    #[test]
    fn tracking_status_with_face_found_emits_face_found_with_hand_bools() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "TrackingStatusChangedEvent",
            serde_json::json!({
                "faceFound": true,
                "leftHandFound": true,
                "rightHandFound": false
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.tracking.face_found")
            .unwrap();
        assert_eq!(ev.source, EventSource::VTube);
        assert_eq!(ev.payload["is_left_hand_found"], true);
        assert_eq!(ev.payload["is_right_hand_found"], false);
    }

    #[test]
    fn tracking_status_without_face_found_emits_face_lost() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "TrackingStatusChangedEvent",
            serde_json::json!({
                "faceFound": false,
                "leftHandFound": false,
                "rightHandFound": true
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.tracking.face_lost")
            .unwrap();
        assert_eq!(ev.payload["is_left_hand_found"], false);
        assert_eq!(ev.payload["is_right_hand_found"], true);
    }

    #[test]
    fn item_event_added_emits_item_added() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ItemEvent",
            serde_json::json!({
                "itemEventType": "Added",
                "itemInstanceID": "inst-1",
                "itemFileName": "crown.png"
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.item.added")
            .unwrap();
        assert_eq!(ev.payload["item_instance_id"], "inst-1");
        assert_eq!(ev.payload["item_file_name"], "crown.png");
    }

    #[test]
    fn item_event_removed_emits_item_removed() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ItemEvent",
            serde_json::json!({
                "itemEventType": "Removed",
                "itemInstanceID": "inst-1",
                "itemFileName": "crown.png"
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.item.removed")
            .unwrap();
        assert_eq!(ev.payload["item_instance_id"], "inst-1");
        assert_eq!(ev.payload["item_file_name"], "crown.png");
    }

    #[test]
    fn item_event_with_unknown_type_produces_no_bus_event() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ItemEvent",
            serde_json::json!({
                "itemEventType": "DroppedPinned",
                "itemInstanceID": "inst-1",
                "itemFileName": "crown.png"
            }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "an unrecognised itemEventType must fall through without emitting"
        );
    }

    #[test]
    fn config_changed_event_carries_model_id_and_name() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ModelConfigChangedEvent",
            serde_json::json!({ "modelID": "m-99", "modelName": "Nova" }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        let events = publisher.events.lock().unwrap();
        let ev = events
            .iter()
            .find(|e| e.kind == "vtube.model.config_changed")
            .unwrap();
        assert_eq!(ev.payload["model_id"], "m-99");
        assert_eq!(ev.payload["model_name"], "Nova");
    }

    #[test]
    fn every_dispatched_event_kind_is_vtube_prefixed() {
        let publisher = MockPublisher::new();
        let envelopes = [
            make_envelope(
                "ModelLoadedEvent",
                serde_json::json!({ "modelLoaded": true }),
            ),
            make_envelope(
                "ModelLoadedEvent",
                serde_json::json!({ "modelLoaded": false }),
            ),
            make_envelope("ModelConfigChangedEvent", serde_json::json!({})),
            make_envelope("HotkeyTriggeredEvent", serde_json::json!({})),
            make_envelope(
                "TrackingStatusChangedEvent",
                serde_json::json!({ "faceFound": true }),
            ),
            make_envelope(
                "TrackingStatusChangedEvent",
                serde_json::json!({ "faceFound": false }),
            ),
            make_envelope("ItemEvent", serde_json::json!({ "itemEventType": "Added" })),
            make_envelope(
                "ItemEvent",
                serde_json::json!({ "itemEventType": "Removed" }),
            ),
        ];

        for env in envelopes {
            dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);
        }

        let events = publisher.events.lock().unwrap();
        assert_eq!(
            events.len(),
            8,
            "each envelope must emit exactly one bus event"
        );
        for ev in events.iter() {
            assert!(
                ev.kind.starts_with("vtube."),
                "emitted kind is not vtube-prefixed: {}",
                ev.kind
            );
        }
    }

    #[test]
    fn expression_activation_event_is_not_dispatched() {
        let publisher = MockPublisher::new();
        let env = make_envelope(
            "ExpressionActivationEvent",
            serde_json::json!({ "expressionFile": "smile.exp3.json", "active": true }),
        );

        dispatch_vts_event(&env, &*Arc::clone(&publisher) as &dyn EventPublisher);

        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "ExpressionActivationEvent has no VTS wire event and must not dispatch"
        );
    }

    #[test]
    fn only_item_spawns_and_removals_change_the_scene_item_lists() {
        for (message_type, item_event_type, expected) in [
            ("ItemEvent", Some("Added"), true),
            ("ItemEvent", Some("Removed"), true),
            ("ItemEvent", Some("DroppedPinned"), false),
            ("ItemEvent", Some("Clicked"), false),
            ("ItemEvent", Some("Locked"), false),
            ("ItemEvent", Some("added"), false),
            ("ItemEvent", None, false),
            ("ModelLoadedEvent", Some("Added"), false),
            ("HotkeyTriggeredEvent", Some("Removed"), false),
        ] {
            let data = match item_event_type {
                Some(kind) => serde_json::json!({ "itemEventType": kind }),
                None => serde_json::json!({}),
            };
            assert_eq!(
                changes_scene_items(&make_envelope(message_type, data)),
                expected,
                "{message_type} {item_event_type:?}"
            );
        }
    }
}
