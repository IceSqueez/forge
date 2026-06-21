use forge_events::{Event, EventPublisher, EventSource};
use serde::Deserialize;
use serde_json::json;

use crate::client::VtsWs;
use crate::error::VTubeError;
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
        "FaceFoundEvent",
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
                    json!({ "model_id": model_id, "model_name": model_name }),
                ));
            } else {
                publisher.publish(Event::new(
                    EventSource::VTube,
                    "model.unloaded",
                    json!({ "model_id": model_id, "model_name": model_name }),
                ));
            }
        }
        "ModelConfigChangedEvent" => {
            let model_name = env.data["modelName"].as_str().unwrap_or("").to_owned();
            publisher.publish(Event::new(
                EventSource::VTube,
                "model.config_changed",
                json!({ "model_name": model_name }),
            ));
        }
        "HotkeyTriggeredEvent" => {
            let hotkey_id = env.data["hotkeyID"].as_str().unwrap_or("").to_owned();
            let hotkey_name = env.data["hotkeyName"].as_str().unwrap_or("").to_owned();
            publisher.publish(Event::new(
                EventSource::VTube,
                "hotkey.triggered",
                json!({ "hotkey_id": hotkey_id, "hotkey_name": hotkey_name }),
            ));
        }
        "ExpressionActivationEvent" => {
            let expression_name = env.data["expressionFile"].as_str().unwrap_or("").to_owned();
            let active = env.data["active"].as_bool().unwrap_or(false);
            publisher.publish(Event::new(
                EventSource::VTube,
                "expression.state_changed",
                json!({ "expression_name": expression_name, "active": active }),
            ));
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
