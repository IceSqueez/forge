use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct SourceSceneItemVisibilityChangedDescriptor;

impl TriggerKindDescriptor for SourceSceneItemVisibilityChangedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.scene_item_visibility_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene item visibility changed"
    }

    fn summary(&self) -> &str {
        "Fires when a scene item's visibility is toggled in OBS."
    }

    fn search_text(&self) -> &str {
        "obs source scene item visible hidden show hide visibility"
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
        "any scene item visibility change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "source.visibility.changed"
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
        if let Some(visible) = event.payload.get("visible").and_then(|v| v.as_bool()) {
            stack = stack.set("obs.source.is_enabled".to_owned(), Variant::Bool(visible));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.scene.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Scene name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Source name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.source.is_enabled".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Source visible".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // 1:1 kind discrimination across the `source.` family (incl. the input/visibility
    // prefix collision) is covered in `source_input_created`. Here we test only this
    // descriptor's typed arg-stack extraction, which renames three distinct payload keys
    // (`scene` / `source` / `visible`) and is the only `source.` descriptor producing a
    // `Variant::Bool`.
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn visibility_arg_stack_extracts_scene_source_and_enabled_flag() {
        let event = Event::new(
            EventSource::Obs,
            "source.visibility.changed",
            json!({ "scene": "Main", "source": "Cam", "visible": true }),
        );
        let stack = SourceSceneItemVisibilityChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Main".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Cam".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.is_enabled"),
            Some(&Variant::Bool(true)),
        );
    }

    /// A `visible: false` payload must map to `Variant::Bool(false)` - not be dropped or
    /// coerced - so actions can distinguish hide from show.
    #[test]
    fn visibility_arg_stack_preserves_false_enabled_flag() {
        let event = Event::new(
            EventSource::Obs,
            "source.visibility.changed",
            json!({ "scene": "Main", "source": "Cam", "visible": false }),
        );
        let stack = SourceSceneItemVisibilityChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.is_enabled"),
            Some(&Variant::Bool(false)),
        );
    }

    #[test]
    fn visibility_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "source.visibility.changed", json!({}));
        let stack = SourceSceneItemVisibilityChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.is_enabled").is_none());
    }
}
