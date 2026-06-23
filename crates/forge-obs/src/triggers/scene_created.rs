use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SceneCreatedDescriptor;

impl TriggerKindDescriptor for SceneCreatedDescriptor {
    fn id(&self) -> &str {
        "obs.scenes.created"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene created"
    }

    fn summary(&self) -> &str {
        "Fires when a new scene is created in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene created added new"
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
        "any scene created".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("scene.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "scene.created"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("scene_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.scene.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    /// The `scene.` prefix is shared by six kinds. `scene.created` must fire on its own
    /// kind and reject every sibling under the same prefix.
    #[test]
    fn matches_only_scene_created_kind_within_scene_family() {
        let d = SceneCreatedDescriptor;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "scene.created", json!({})),
        ));
        for sibling in [
            "scene.removed",
            "scene.renamed",
            "scene.changed",
            "scene.preview_changed",
            "scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "scene.created wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_scene_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "scene.created",
            json!({ "scene_name": "BRB" }),
        );
        let stack = SceneCreatedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("BRB".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "scene.created", json!({}));
        let stack = SceneCreatedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
    }
}
