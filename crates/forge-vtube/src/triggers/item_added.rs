use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct ItemAddedDescriptor;

impl TriggerKindDescriptor for ItemAddedDescriptor {
    fn id(&self) -> &str {
        "vtube.item.added"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio item added"
    }

    fn summary(&self) -> &str {
        "Fires when an item is loaded into the VTube Studio scene."
    }

    fn search_text(&self) -> &str {
        "vtube item added loaded prop sticker"
    }

    fn icon_name(&self) -> &str {
        "image"
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
        "any item".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("item.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "item.added"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(id) = event
            .payload
            .get("item_instance_id")
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "vtube.item.instance_id".to_owned(),
                Variant::String(id.to_owned()),
            );
        }
        if let Some(file) = event.payload.get("item_file_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "vtube.item.file_name".to_owned(),
                Variant::String(file.to_owned()),
            );
        }
        stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::VTube, kind, payload)
    }

    #[test]
    fn matches_only_the_exact_added_kind() {
        let d = ItemAddedDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("item.added", json!({}))));
        // Sibling under the same `item.` prefix must not match.
        assert!(!d.matches_trigger(&cfg, &event("item.removed", json!({}))));
        // Foreign kind.
        assert!(!d.matches_trigger(&cfg, &event("model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = ItemAddedDescriptor;
        let stack = d.build_arg_stack(&event(
            "item.added",
            json!({ "item_instance_id": "inst-7", "item_file_name": "hat.png" }),
        ));
        assert_eq!(
            stack.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-7".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.item.file_name"),
            Some(&Variant::String("hat.png".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = ItemAddedDescriptor;
        let stack = d.build_arg_stack(&event(
            "item.added",
            json!({ "item_instance_id": "inst-7" }),
        ));
        assert_eq!(
            stack.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-7".to_owned()))
        );
        assert!(stack.get("vtube.item.file_name").is_none());
    }
}
