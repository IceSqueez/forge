use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct ItemRemovedDescriptor;

impl TriggerKindDescriptor for ItemRemovedDescriptor {
    fn id(&self) -> &str {
        "vtube.item.removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio item removed"
    }

    fn summary(&self) -> &str {
        "Fires when an item is removed from the VTube Studio scene."
    }

    fn search_text(&self) -> &str {
        "vtube item removed unloaded prop sticker"
    }

    fn icon_name(&self) -> &str {
        "image-off"
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
        event.kind == "item.removed"
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
