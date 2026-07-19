use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

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

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.scene_names".to_owned(),
                kind: VariantKind::Array,
                label: "All scene names".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_list_changed_kind() {
        let d = SceneListChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "scene.list_changed",
            json!({ "all_names": ["Menu", "Gameplay"] }),
        );
        assert!(d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn does_not_match_other_scene_kind() {
        let d = SceneListChangedDescriptor;
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_collects_all_names_into_string_array() {
        let d = SceneListChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "scene.list_changed",
            json!({ "all_names": ["Menu", "Gameplay", "BRB"] }),
        );
        let stack = d.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene_names"),
            Some(&Variant::Array(vec![
                Variant::String("Menu".to_owned()),
                Variant::String("Gameplay".to_owned()),
                Variant::String("BRB".to_owned()),
            ]))
        );
    }

    #[test]
    fn build_arg_stack_skips_non_string_array_entries() {
        let d = SceneListChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "scene.list_changed",
            json!({ "all_names": ["Menu", 7, "Gameplay"] }),
        );
        let stack = d.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene_names"),
            Some(&Variant::Array(vec![
                Variant::String("Menu".to_owned()),
                Variant::String("Gameplay".to_owned()),
            ]))
        );
    }

    #[test]
    fn build_arg_stack_omits_key_when_all_names_missing() {
        let d = SceneListChangedDescriptor;
        let event = Event::new(EventSource::Obs, "scene.list_changed", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.scene_names"), None);
    }
}
