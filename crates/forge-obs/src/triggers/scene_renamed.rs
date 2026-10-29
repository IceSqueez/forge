use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::scene as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub struct SceneRenamedDescriptor;

impl TriggerKindDescriptor for SceneRenamedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::SCENE_RENAMED
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
            kind_prefix: Some(event_kinds::SCENE_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::SCENE_RENAMED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(old) = event
            .payload
            .get(fields::SCENE_NAME_OLD)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::scene::NAME_OLD.to_owned(),
                Variant::String(old.to_owned()),
            );
        }
        if let Some(new) = event
            .payload
            .get(fields::SCENE_NAME_NEW)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::scene::NAME_NEW.to_owned(),
                Variant::String(new.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: variables::scene::NAME_OLD.to_owned(),
                    kind: VariantKind::String,
                    label: "Old scene name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::scene::NAME_NEW.to_owned(),
                    kind: VariantKind::String,
                    label: "New scene name".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
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
            &Event::new(EventSource::Obs, "obs.scene.renamed", json!({})),
        ));
        for sibling in [
            "obs.scene.created",
            "obs.scene.removed",
            "obs.scene.changed",
            "obs.scene.preview_changed",
            "obs.scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.scene.renamed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_old_and_new_names_to_distinct_keys() {
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.renamed",
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
            "obs.scene.renamed",
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
