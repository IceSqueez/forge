use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::scene as fields;

pub struct SceneRemovedDescriptor;

impl TriggerKindDescriptor for SceneRemovedDescriptor {
    fn id(&self) -> &str {
        "obs.scenes.removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene removed"
    }

    fn summary(&self) -> &str {
        "Fires when a scene is deleted in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene removed deleted"
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
        "any scene removed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("scene.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "scene.removed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::SCENE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.scene.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.scene.name".to_owned(),
                kind: VariantKind::String,
                label: "Scene name".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn matches_only_scene_removed_kind_within_scene_family() {
        let d = SceneRemovedDescriptor;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "scene.removed", json!({})),
        ));
        for sibling in [
            "scene.created",
            "scene.renamed",
            "scene.changed",
            "scene.preview_changed",
            "scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "scene.removed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_scene_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "scene.removed",
            json!({ "scene_name": "Intro" }),
        );
        let stack = SceneRemovedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Intro".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "scene.removed", json!({}));
        let stack = SceneRemovedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
    }
}
