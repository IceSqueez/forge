use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct ScenePreviewChangedDescriptor;

impl TriggerKindDescriptor for ScenePreviewChangedDescriptor {
    fn id(&self) -> &str {
        "obs.scenes.preview_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS preview scene changed"
    }

    fn summary(&self) -> &str {
        "Fires when the Studio Mode preview scene changes."
    }

    fn search_text(&self) -> &str {
        "obs preview scene changed studio mode"
    }

    fn icon_name(&self) -> &str {
        "layers-subtract"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Optional {
            key: "scene",
            label: "Scene name (leave empty to match any)",
            inner: Box::new(FormField::DynamicSelect {
                key: "scene",
                label: "Scene",
                options_key: "obs.scene_names",
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        match config.get("scene") {
            Some(Variant::String(s)) if !s.is_empty() => format!("preview = {s}"),
            _ => "any preview scene".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("scene.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "scene.preview_changed" {
            return false;
        }
        match config.get("scene") {
            Some(Variant::String(s)) if !s.is_empty() => {
                event.payload.get("name_new").and_then(|v| v.as_str()) == Some(s.as_str())
            }
            _ => true,
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(new) = event.payload.get("name_new").and_then(|v| v.as_str()) {
            stack = stack.set("obs.scene".to_owned(), Variant::String(new.to_owned()));
        }
        if let Some(old) = event.payload.get("name_old").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.previous_scene".to_owned(),
                Variant::String(old.to_owned()),
            );
        }
        stack
    }
}
