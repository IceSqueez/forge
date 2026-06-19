use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SceneListChangedDescriptor;

impl TriggerKindDescriptor for SceneListChangedDescriptor {
    fn id(&self) -> &str {
        "obs.scenes.list_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene list changed"
    }

    fn summary(&self) -> &str {
        "Fires when scenes are added, removed, or reordered in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene list changed added removed reordered"
    }

    fn icon_name(&self) -> &str {
        "layout-grid"
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
        "any scene list change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("scene.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "scene.list_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(names) = event.payload.get("all_names").and_then(|v| v.as_array()) {
            let scenes: Vec<Variant> = names
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| Variant::String(s.to_owned()))
                .collect();
            stack = stack.set("obs.scene_names".to_owned(), Variant::Array(scenes));
        }
        stack
    }
}
