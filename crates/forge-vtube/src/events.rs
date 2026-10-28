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
    use serde_json::json;

    use super::*;
    use crate::client::tests::MockPublisher;

    fn model_loaded(loaded: bool) -> VtsEvent {
        VtsEvent::ModelLoaded {
            loaded,
            model_id: "model-abc".to_owned(),
            model_name: "MyAvatar".to_owned(),
        }
    }

    fn tracking(face_found: bool) -> VtsEvent {
        VtsEvent::TrackingStatusChanged {
            face_found,
            left_hand_found: true,
            right_hand_found: false,
        }
    }

    fn item(change: ItemChange) -> VtsEvent {
        VtsEvent::ItemChanged {
            change,
            item_instance_id: "inst-1".to_owned(),
            item_file_name: "crown.png".to_owned(),
        }
    }

    #[test]
    fn each_vts_event_publishes_exactly_one_vtube_event_with_its_kind_and_payload() {
        for (event, kind, payload) in [
            (
                model_loaded(true),
                "vtube.model.loaded",
                json!({ "model_id": "model-abc", "model_name": "MyAvatar" }),
            ),
            (
                model_loaded(false),
                "vtube.model.unloaded",
                json!({ "model_id": "model-abc", "model_name": "MyAvatar" }),
            ),
            (
                VtsEvent::ModelConfigChanged {
                    model_id: "m-99".to_owned(),
                    model_name: "Nova".to_owned(),
                },
                "vtube.model.config_changed",
                json!({ "model_id": "m-99", "model_name": "Nova" }),
            ),
            (
                VtsEvent::HotkeyTriggered {
                    hotkey_id: "hk-001".to_owned(),
                    hotkey_name: "BlushExpression".to_owned(),
                },
                "vtube.hotkey.triggered",
                json!({ "hotkey_id": "hk-001", "hotkey_name": "BlushExpression" }),
            ),
            (
                tracking(true),
                "vtube.tracking.face_found",
                json!({ "is_left_hand_found": true, "is_right_hand_found": false }),
            ),
            (
                tracking(false),
                "vtube.tracking.face_lost",
                json!({ "is_left_hand_found": true, "is_right_hand_found": false }),
            ),
            (
                item(ItemChange::Added),
                "vtube.item.added",
                json!({ "item_instance_id": "inst-1", "item_file_name": "crown.png" }),
            ),
            (
                item(ItemChange::Removed),
                "vtube.item.removed",
                json!({ "item_instance_id": "inst-1", "item_file_name": "crown.png" }),
            ),
        ] {
            let publisher = MockPublisher::new();

            dispatch_vts_event(&event, publisher.as_ref());

            let events = publisher.events.lock().unwrap();
            let [published] = events.as_slice() else {
                panic!(
                    "{kind}: expected exactly one bus event, got {}",
                    events.len()
                );
            };
            assert_eq!(published.source, EventSource::VTube, "{kind}");
            assert_eq!(published.kind, kind);
            assert_eq!(published.payload, payload, "{kind}");
        }
    }
}
