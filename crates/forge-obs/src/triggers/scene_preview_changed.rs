use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

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

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.scene".to_owned(),
                    kind: VariantKind::String,
                    label: "Scene name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.previous_scene".to_owned(),
                    kind: VariantKind::String,
                    label: "Previous scene name".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn preview_event(old: &str, new: &str) -> Event {
        Event::new(
            EventSource::Obs,
            "scene.preview_changed",
            json!({ "name_old": old, "name_new": new }),
        )
    }

    fn named_config(scene: &str) -> TriggerConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(scene.to_owned()))])
    }

    #[test]
    fn matches_preview_changed_kind_with_empty_config() {
        let d = ScenePreviewChangedDescriptor;
        assert!(d.matches_trigger(&BTreeMap::new(), &preview_event("Menu", "Gameplay")));
    }

    #[test]
    fn matches_when_config_scene_equals_new_preview() {
        let d = ScenePreviewChangedDescriptor;
        assert!(d.matches_trigger(
            &named_config("Gameplay"),
            &preview_event("Menu", "Gameplay")
        ));
    }

    #[test]
    fn does_not_match_when_config_scene_differs_from_new_preview() {
        let d = ScenePreviewChangedDescriptor;
        assert!(!d.matches_trigger(&named_config("BRB"), &preview_event("Menu", "Gameplay")));
    }

    #[test]
    fn does_not_match_program_scene_changed_kind() {
        let d = ScenePreviewChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "scene.changed",
            json!({ "name_new": "Gameplay" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_maps_new_to_scene_and_old_to_previous() {
        let d = ScenePreviewChangedDescriptor;
        let stack = d.build_arg_stack(&preview_event("Menu", "Gameplay"));
        assert_eq!(
            stack.get("obs.scene"),
            Some(&Variant::String("Gameplay".to_owned()))
        );
        assert_eq!(
            stack.get("obs.previous_scene"),
            Some(&Variant::String("Menu".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_previous_scene_when_name_old_missing() {
        let d = ScenePreviewChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "scene.preview_changed",
            json!({ "name_new": "Gameplay" }),
        );
        let stack = d.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene"),
            Some(&Variant::String("Gameplay".to_owned()))
        );
        assert_eq!(stack.get("obs.previous_scene"), None);
    }
}
