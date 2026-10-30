use std::collections::HashMap;

use serde_json::{Value, json};

use crate::error::VTubeError;
use crate::vts::message_type::{
    EVENT_SUBSCRIPTION_REQUEST, HOTKEY_TRIGGERED_EVENT, ITEM_EVENT, MODEL_CONFIG_CHANGED_EVENT,
    MODEL_LOADED_EVENT, TRACKING_STATUS_CHANGED_EVENT,
};
use crate::vts::protocol::new_request;
use crate::vts::socket::{VtsWs, send_ws_msg};

const SUBSCRIBED_EVENTS: [&str; 5] = [
    MODEL_LOADED_EVENT,
    MODEL_CONFIG_CHANGED_EVENT,
    HOTKEY_TRIGGERED_EVENT,
    TRACKING_STATUS_CHANGED_EVENT,
    ITEM_EVENT,
];

const ITEM_EVENT_ADDED: &str = "Added";
const ITEM_EVENT_REMOVED: &str = "Removed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemChange {
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VtsEvent {
    ModelLoaded {
        loaded: bool,
        model_id: String,
        model_name: String,
    },
    ModelConfigChanged {
        model_id: String,
        model_name: String,
    },
    HotkeyTriggered {
        hotkey_id: String,
        hotkey_name: String,
    },
    TrackingStatusChanged {
        face_found: bool,
        left_hand_found: bool,
        right_hand_found: bool,
    },
    ItemChanged {
        change: ItemChange,
        item_instance_id: String,
        item_file_name: String,
    },
}

impl VtsEvent {
    pub(crate) fn decode(envelope: &Value) -> Option<Self> {
        let message_type = envelope.get("messageType")?.as_str()?;
        let data = envelope.get("data")?;
        let event = match message_type {
            MODEL_LOADED_EVENT => Self::ModelLoaded {
                loaded: flag(data, "modelLoaded"),
                model_id: text(data, "modelID"),
                model_name: text(data, "modelName"),
            },
            MODEL_CONFIG_CHANGED_EVENT => Self::ModelConfigChanged {
                model_id: text(data, "modelID"),
                model_name: text(data, "modelName"),
            },
            HOTKEY_TRIGGERED_EVENT => Self::HotkeyTriggered {
                hotkey_id: text(data, "hotkeyID"),
                hotkey_name: text(data, "hotkeyName"),
            },
            TRACKING_STATUS_CHANGED_EVENT => Self::TrackingStatusChanged {
                face_found: flag(data, "faceFound"),
                left_hand_found: flag(data, "leftHandFound"),
                right_hand_found: flag(data, "rightHandFound"),
            },
            ITEM_EVENT => Self::ItemChanged {
                change: match data["itemEventType"].as_str() {
                    Some(ITEM_EVENT_ADDED) => ItemChange::Added,
                    Some(ITEM_EVENT_REMOVED) => ItemChange::Removed,
                    _ => return None,
                },
                item_instance_id: text(data, "itemInstanceID"),
                item_file_name: text(data, "itemFileName"),
            },
            _ => return None,
        };
        Some(event)
    }

    pub(crate) fn changes_scene_items(&self) -> bool {
        matches!(self, Self::ItemChanged { .. })
    }
}

fn text(data: &Value, key: &str) -> String {
    data[key].as_str().unwrap_or("").to_owned()
}

fn flag(data: &Value, key: &str) -> bool {
    data[key].as_bool().unwrap_or(false)
}

pub(crate) async fn subscribe_all(
    ws: &mut VtsWs,
) -> Result<HashMap<String, &'static str>, VTubeError> {
    let mut requested = HashMap::with_capacity(SUBSCRIBED_EVENTS.len());
    for event_name in SUBSCRIBED_EVENTS {
        let req = new_request(
            EVENT_SUBSCRIPTION_REQUEST,
            json!({ "eventName": event_name, "subscribe": true }),
        );
        send_ws_msg(ws, &req).await?;
        requested.insert(req.request_id, event_name);
    }
    Ok(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(message_type: &str, data: Value) -> Value {
        json!({ "messageType": message_type, "data": data })
    }

    #[test]
    fn each_subscribed_event_decodes_into_its_variant_with_the_wire_fields() {
        for (message_type, data, expected) in [
            (
                "ModelLoadedEvent",
                json!({ "modelLoaded": true, "modelID": "m-1", "modelName": "Avatar" }),
                VtsEvent::ModelLoaded {
                    loaded: true,
                    model_id: "m-1".to_owned(),
                    model_name: "Avatar".to_owned(),
                },
            ),
            (
                "ModelConfigChangedEvent",
                json!({ "modelID": "m-2", "modelName": "Nova" }),
                VtsEvent::ModelConfigChanged {
                    model_id: "m-2".to_owned(),
                    model_name: "Nova".to_owned(),
                },
            ),
            (
                "HotkeyTriggeredEvent",
                json!({ "hotkeyID": "hk-1", "hotkeyName": "Blush" }),
                VtsEvent::HotkeyTriggered {
                    hotkey_id: "hk-1".to_owned(),
                    hotkey_name: "Blush".to_owned(),
                },
            ),
            (
                "TrackingStatusChangedEvent",
                json!({ "faceFound": true, "leftHandFound": false, "rightHandFound": true }),
                VtsEvent::TrackingStatusChanged {
                    face_found: true,
                    left_hand_found: false,
                    right_hand_found: true,
                },
            ),
            (
                "ItemEvent",
                json!({ "itemEventType": "Added", "itemInstanceID": "i-1", "itemFileName": "crown.png" }),
                VtsEvent::ItemChanged {
                    change: ItemChange::Added,
                    item_instance_id: "i-1".to_owned(),
                    item_file_name: "crown.png".to_owned(),
                },
            ),
            (
                "ItemEvent",
                json!({ "itemEventType": "Removed", "itemInstanceID": "i-2", "itemFileName": "hat.png" }),
                VtsEvent::ItemChanged {
                    change: ItemChange::Removed,
                    item_instance_id: "i-2".to_owned(),
                    item_file_name: "hat.png".to_owned(),
                },
            ),
        ] {
            assert_eq!(
                VtsEvent::decode(&envelope(message_type, data)),
                Some(expected),
                "{message_type}"
            );
        }
    }

    #[test]
    fn missing_or_mistyped_fields_decode_to_empty_text_and_false() {
        for (message_type, data, expected) in [
            (
                "ModelLoadedEvent",
                json!({}),
                VtsEvent::ModelLoaded {
                    loaded: false,
                    model_id: String::new(),
                    model_name: String::new(),
                },
            ),
            (
                "ModelLoadedEvent",
                json!({ "modelLoaded": "true", "modelID": 7, "modelName": null }),
                VtsEvent::ModelLoaded {
                    loaded: false,
                    model_id: String::new(),
                    model_name: String::new(),
                },
            ),
            (
                "HotkeyTriggeredEvent",
                json!({ "hotkeyID": ["hk"], "hotkeyName": { "n": 1 } }),
                VtsEvent::HotkeyTriggered {
                    hotkey_id: String::new(),
                    hotkey_name: String::new(),
                },
            ),
            (
                "TrackingStatusChangedEvent",
                json!({ "faceFound": 1, "leftHandFound": "yes" }),
                VtsEvent::TrackingStatusChanged {
                    face_found: false,
                    left_hand_found: false,
                    right_hand_found: false,
                },
            ),
            (
                "ItemEvent",
                json!({ "itemEventType": "Added", "itemInstanceID": 3 }),
                VtsEvent::ItemChanged {
                    change: ItemChange::Added,
                    item_instance_id: String::new(),
                    item_file_name: String::new(),
                },
            ),
            (
                "ModelConfigChangedEvent",
                Value::Null,
                VtsEvent::ModelConfigChanged {
                    model_id: String::new(),
                    model_name: String::new(),
                },
            ),
        ] {
            assert_eq!(
                VtsEvent::decode(&envelope(message_type, data.clone())),
                Some(expected),
                "{message_type} {data}"
            );
        }
    }

    #[test]
    fn item_events_other_than_spawn_and_removal_are_not_decoded() {
        for data in [
            json!({ "itemEventType": "DroppedPinned", "itemInstanceID": "i-1" }),
            json!({ "itemEventType": "Clicked" }),
            json!({ "itemEventType": "added" }),
            json!({ "itemEventType": true }),
            json!({ "itemInstanceID": "i-1" }),
            Value::Null,
        ] {
            assert_eq!(
                VtsEvent::decode(&envelope("ItemEvent", data.clone())),
                None,
                "{data}"
            );
        }
    }

    #[test]
    fn envelopes_that_are_not_a_subscribed_event_are_not_decoded() {
        for frame in [
            envelope("ExpressionActivationEvent", json!({ "active": true })),
            envelope("CurrentModelResponse", json!({ "modelLoaded": true })),
            envelope("modelloadedevent", json!({ "modelLoaded": true })),
            json!({ "messageType": "ModelLoadedEvent" }),
            json!({ "messageType": 7, "data": {} }),
            json!({ "data": { "modelLoaded": true } }),
            json!("ModelLoadedEvent"),
            Value::Null,
        ] {
            assert_eq!(VtsEvent::decode(&frame), None, "{frame}");
        }
    }
}
