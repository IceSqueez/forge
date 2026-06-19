use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SceneCollectionChangingDescriptor;

impl TriggerKindDescriptor for SceneCollectionChangingDescriptor {
    fn id(&self) -> &str {
        "obs.collection.changing"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene collection changing"
    }

    fn summary(&self) -> &str {
        "Fires when OBS begins switching to a different scene collection."
    }

    fn search_text(&self) -> &str {
        "obs scene collection changing switching"
    }

    fn icon_name(&self) -> &str {
        "stack-2"
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
        "any scene collection".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("collection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "collection.changing"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.collection".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }
}
