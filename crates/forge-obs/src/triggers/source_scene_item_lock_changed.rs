use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SourceSceneItemLockChangedDescriptor;

impl TriggerKindDescriptor for SourceSceneItemLockChangedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.scene_item_lock_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene item lock changed"
    }

    fn summary(&self) -> &str {
        "Fires when a scene item's lock state is toggled in OBS."
    }

    fn search_text(&self) -> &str {
        "obs source scene item lock locked unlock"
    }

    fn icon_name(&self) -> &str {
        "lock"
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
        "any scene item lock change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "source.scene_item_lock_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(scene) = event.payload.get("scene").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.scene.name".to_owned(),
                Variant::String(scene.to_owned()),
            );
        }
        if let Some(source) = event.payload.get("source").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(source.to_owned()),
            );
        }
        if let Some(locked) = event.payload.get("is_locked").and_then(|v| v.as_bool()) {
            stack = stack.set("obs.source.is_locked".to_owned(), Variant::Bool(locked));
        }
        stack
    }
}
