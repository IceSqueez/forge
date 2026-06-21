use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct FaceFoundDescriptor;

impl TriggerKindDescriptor for FaceFoundDescriptor {
    fn id(&self) -> &str {
        "vtube.tracking.face_found"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio face found"
    }

    fn summary(&self) -> &str {
        "Fires when VTube Studio starts tracking a face."
    }

    fn search_text(&self) -> &str {
        "vtube tracking face found detected webcam"
    }

    fn icon_name(&self) -> &str {
        "eye"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "face found".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("tracking.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "tracking.face_found"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(left) = event
            .payload
            .get("left_hand_found")
            .and_then(|v| v.as_bool())
        {
            stack = stack.set(
                "vtube.tracking.left_hand_found".to_owned(),
                Variant::Bool(left),
            );
        }
        if let Some(right) = event
            .payload
            .get("right_hand_found")
            .and_then(|v| v.as_bool())
        {
            stack = stack.set(
                "vtube.tracking.right_hand_found".to_owned(),
                Variant::Bool(right),
            );
        }
        stack
    }
}
