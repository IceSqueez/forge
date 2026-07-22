use forge_events::{Event, EventPublisher, EventSource};
use serde::Deserialize;
use serde_json::json;

use crate::client::VtsWs;
use crate::error::VTubeError;
use crate::payload_fields::{
    expression as expression_fields, hotkey as hotkey_fields, item as item_fields,
    model as model_fields, tracking as tracking_fields,
};
use crate::protocol::new_request;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawEnvelope {
    #[serde(rename = "messageType")]
    pub message_type: String,
    pub data: serde_json::Value,
}

pub(crate) async fn subscribe_all(ws: &mut VtsWs) -> Result<(), VTubeError> {
    for event_name in [
        "ModelLoadedEvent",
        "ModelConfigChangedEvent",
        "HotkeyTriggeredEvent",
        "ExpressionActivationEvent",
        "TrackingStatusChangedEvent",
        "ItemEvent",
    ] {
        let req = new_request(
            "EventSubscriptionRequest",
            json!({ "eventName": event_name, "subscribe": true }),
        );
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite::Message;
        let text = serde_json::to_string(&req).map_err(VTubeError::Json)?;
        ws.send(Message::Text(text.into()))
            .await
            .map_err(|e| VTubeError::Connect(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn dispatch_vts_event(env: &RawEnvelope, publisher: &dyn EventPublisher) {
    match env.message_type.as_str() {
        "ModelLoadedEvent" => {
            let loaded = env.data["modelLoaded"].as_bool().unwrap_or(false);
            let model_id = env.data["modelID"].as_str().unwrap_or("").to_owned();
            let model_name = env.data["modelName"].as_str().unwrap_or("").to_owned();
            if loaded {
                publisher.publish(Event::new(
                    EventSource::VTube,
                    "model.loaded",
                    json!({ (model_fields::MODEL_ID): model_id, (model_fields::MODEL_NAME): model_name }),
                ));
            } else {
                publisher.publish(Event::new(
                    EventSource::VTube,
                    "model.unloaded",
                    json!({ (model_fields::MODEL_ID): model_id, (model_fields::MODEL_NAME): model_name }),
                ));
            }
        }
        "ModelConfigChangedEvent" => {
            let model_name = env.data["modelName"].as_str().unwrap_or("").to_owned();
            publisher.publish(Event::new(
                EventSource::VTube,
                "model.config_changed",
                json!({ (model_fields::MODEL_NAME): model_name }),
            ));
        }
        "HotkeyTriggeredEvent" => {
            let hotkey_id = env.data["hotkeyID"].as_str().unwrap_or("").to_owned();
            let hotkey_name = env.data["hotkeyName"].as_str().unwrap_or("").to_owned();
            publisher.publish(Event::new(
                EventSource::VTube,
                "hotkey.triggered",
                json!({ (hotkey_fields::HOTKEY_ID): hotkey_id, (hotkey_fields::HOTKEY_NAME): hotkey_name }),
            ));
        }
        "ExpressionActivationEvent" => {
            let expression_name = env.data["expressionFile"].as_str().unwrap_or("").to_owned();
            let active = env.data["active"].as_bool().unwrap_or(false);
            publisher.publish(Event::new(
                EventSource::VTube,
                "expression.state_changed",
                json!({ (expression_fields::EXPRESSION_NAME): expression_name, (expression_fields::ACTIVE): active }),
            ));
        }
        "TrackingStatusChangedEvent" => {
            let face_found = env.data["faceFound"].as_bool().unwrap_or(false);
            let left_hand_found = env.data["leftHandFound"].as_bool().unwrap_or(false);
            let right_hand_found = env.data["rightHandFound"].as_bool().unwrap_or(false);
            let kind = if face_found {
                "tracking.face_found"
            } else {
                "tracking.face_lost"
            };
            publisher.publish(Event::new(
                EventSource::VTube,
                kind,
                json!({
                    (tracking_fields::LEFT_HAND_FOUND): left_hand_found,
                    (tracking_fields::RIGHT_HAND_FOUND): right_hand_found,
                }),
            ));
        }
        "ItemEvent" => {
            let item_event_type = env.data["itemEventType"].as_str().unwrap_or("");
            let kind = match item_event_type {
                "Added" => Some("item.added"),
                "Removed" => Some("item.removed"),
                _ => None,
            };
            if let Some(kind) = kind {
                let item_instance_id = env.data["itemInstanceID"].as_str().unwrap_or("").to_owned();
                let item_file_name = env.data["itemFileName"].as_str().unwrap_or("").to_owned();
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
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        let ev = events.iter().find(|e| e.kind == "model.loaded").unwrap();
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
        let ev = events.iter().find(|e| e.kind == "model.unloaded").unwrap();
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
            .find(|e| e.kind == "hotkey.triggered")
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
            .find(|e| e.kind == "tracking.face_found")
            .unwrap();
        assert_eq!(ev.source, EventSource::VTube);
        assert_eq!(ev.payload["left_hand_found"], true);
        assert_eq!(ev.payload["right_hand_found"], false);
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
            .find(|e| e.kind == "tracking.face_lost")
            .unwrap();
        assert_eq!(ev.payload["left_hand_found"], false);
        assert_eq!(ev.payload["right_hand_found"], true);
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
        let ev = events.iter().find(|e| e.kind == "item.added").unwrap();
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
        let ev = events.iter().find(|e| e.kind == "item.removed").unwrap();
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
    fn raw_envelope_serde_roundtrip() {
        let raw = r#"{
            "requestID": "req-001",
            "apiName": "VTubeStudioPublicAPI",
            "apiVersion": "1.0",
            "messageType": "HotkeyTriggeredEvent",
            "data": { "hotkeyID": "hk-abc", "hotkeyName": "Wave" }
        }"#;
        let env: RawEnvelope = serde_json::from_str(raw).unwrap();
        assert_eq!(env.message_type, "HotkeyTriggeredEvent");
        assert_eq!(env.data["hotkeyID"], "hk-abc");
        assert_eq!(env.data["hotkeyName"], "Wave");
    }
}
