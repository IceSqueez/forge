use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::scene as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub struct SceneRemovedDescriptor;

impl TriggerKindDescriptor for SceneRemovedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::SCENE_REMOVED
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

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_ANY_SCENE_REMOVED.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::SCENE_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::SCENE_REMOVED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::SCENE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::scene::NAME.to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: variables::scene::NAME.to_owned(),
                kind: VariantKind::String,
                label: "Scene name".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
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
            &Event::new(EventSource::Obs, "obs.scene.removed", json!({})),
        ));
        for sibling in [
            "obs.scene.created",
            "obs.scene.renamed",
            "obs.scene.changed",
            "obs.scene.preview_changed",
            "obs.scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.scene.removed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_scene_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.removed",
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
        let event = Event::new(EventSource::Obs, "obs.scene.removed", json!({}));
        let stack = SceneRemovedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
    }
}
