use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

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

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.scene.name_old".to_owned(),
                    kind: VariantKind::String,
                    label: "Old scene name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.scene.name_new".to_owned(),
                    kind: VariantKind::String,
                    label: "New scene name".to_owned(),
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
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn matches_only_scene_renamed_kind_within_scene_family() {
        let d = SceneRenamedDescriptor;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "scene.renamed", json!({})),
        ));
        for sibling in [
            "scene.created",
            "scene.removed",
            "scene.changed",
            "scene.preview_changed",
            "scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "scene.renamed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_old_and_new_names_to_distinct_keys() {
        let event = Event::new(
            EventSource::Obs,
            "scene.renamed",
            json!({ "scene_name_old": "Old", "scene_name_new": "New" }),
        );
        let stack = SceneRenamedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name_old"),
            Some(&Variant::String("Old".to_owned())),
        );
        assert_eq!(
            stack.get("obs.scene.name_new"),
            Some(&Variant::String("New".to_owned())),
        );
    }

    #[test]
    fn arg_stack_binds_only_present_name_field() {
        let event = Event::new(
            EventSource::Obs,
            "scene.renamed",
            json!({ "scene_name_new": "New" }),
        );
        let stack = SceneRenamedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name_old").is_none());
        assert_eq!(
            stack.get("obs.scene.name_new"),
            Some(&Variant::String("New".to_owned())),
        );
    }
}
