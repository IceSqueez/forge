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

pub(crate) async fn subscribe_all(ws: &mut VtsWs) -> Result<(), VTubeError> {
    for event_name in SUBSCRIBED_EVENTS {
        let req = new_request(
            EVENT_SUBSCRIPTION_REQUEST,
            json!({ "eventName": event_name, "subscribe": true }),
        );
        send_ws_msg(ws, &req).await?;
    }
    Ok(())
}
