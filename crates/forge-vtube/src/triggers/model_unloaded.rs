use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct ModelUnloadedDescriptor;

impl TriggerKindDescriptor for ModelUnloadedDescriptor {
    fn id(&self) -> &str {
        "vtube.model.unloaded"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio model unloaded"
    }

    fn summary(&self) -> &str {
        "Fires when a VTube Studio model is unloaded."
    }

    fn search_text(&self) -> &str {
        "vtube model unloaded removed avatar"
    }

    fn icon_name(&self) -> &str {
        "user-minus"
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
        "any model".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("model.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "model.unloaded"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("model_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "vtube.model.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(id) = event.payload.get("model_id").and_then(|v| v.as_str()) {
            stack = stack.set("vtube.model.id".to_owned(), Variant::String(id.to_owned()));
        }
        stack
    }
}
