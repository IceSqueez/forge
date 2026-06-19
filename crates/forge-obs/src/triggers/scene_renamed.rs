use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SceneRenamedDescriptor;

impl TriggerKindDescriptor for SceneRenamedDescriptor {
    fn id(&self) -> &str {
        "obs.scenes.renamed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene renamed"
    }

    fn summary(&self) -> &str {
        "Fires when a scene is renamed in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene renamed name changed"
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
        "any scene renamed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("scene.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "scene.renamed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(old) = event.payload.get("scene_name_old").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.scene.name_old".to_owned(),
                Variant::String(old.to_owned()),
            );
        }
        if let Some(new) = event.payload.get("scene_name_new").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.scene.name_new".to_owned(),
                Variant::String(new.to_owned()),
            );
        }
        stack
    }
}
