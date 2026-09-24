use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::source as fields;

pub struct SourceSceneItemRemovedDescriptor;

impl TriggerKindDescriptor for SourceSceneItemRemovedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.scene_item_removed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene item removed"
    }

    fn summary(&self) -> &str {
        "Fires when a source is removed from a scene in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene item source removed deleted"
    }

    fn icon_name(&self) -> &str {
        "minus-circle"
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
        "any scene item removed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.source.scene_item_removed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(scene) = event
            .payload
            .get(fields::SCENE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.scene.name".to_owned(),
                Variant::String(scene.to_owned()),
            );
        }
        if let Some(source) = event
            .payload
            .get(fields::SOURCE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(source.to_owned()),
            );
        }
        if let Some(item_id) = event.payload.get(fields::ITEM_ID).and_then(|v| v.as_i64()) {
            stack = stack.set("obs.source.item_id".to_owned(), Variant::Int(item_id));
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
                    name: "obs.source.item_id".to_owned(),
                    kind: VariantKind::Int,
                    label: "Scene item ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::Obs, kind, payload)
    }

    #[test]
    fn fires_on_the_scene_item_removed_kind_only() {
        let cfg = TriggerConfig::new();
        for (kind, expected) in [
            ("obs.source.scene_item_removed", true),
            ("obs.source.scene_item_created", false),
            ("obs.source.input_removed", false),
            ("obs.scene.removed", false),
        ] {
            assert_eq!(
                SourceSceneItemRemovedDescriptor.matches_trigger(&cfg, &event(kind, json!({}))),
                expected,
                "{kind}"
            );
        }
    }

    #[test]
    fn arg_stack_carries_the_scene_source_and_item_id() {
        let stack = SourceSceneItemRemovedDescriptor.build_arg_stack(&event(
            "obs.source.scene_item_removed",
            json!({ "scene_name": "Gameplay", "source_name": "Webcam", "item_id": 42 }),
        ));

        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Gameplay".to_owned()))
        );
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Webcam".to_owned()))
        );
        assert_eq!(stack.get("obs.source.item_id"), Some(&Variant::Int(42)));
    }

    #[test]
    fn arg_stack_omits_every_key_whose_payload_field_is_absent_or_mistyped() {
        let stack = SourceSceneItemRemovedDescriptor.build_arg_stack(&event(
            "obs.source.scene_item_removed",
            json!({ "scene_name": 7, "item_id": "42" }),
        ));

        for key in ["obs.scene.name", "obs.source.name", "obs.source.item_id"] {
            assert!(stack.get(key).is_none(), "{key}");
        }
    }
}
