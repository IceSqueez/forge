use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::source as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub struct SourceSceneItemLockChangedDescriptor;

impl TriggerKindDescriptor for SourceSceneItemLockChangedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::SOURCE_LOCK_CHANGED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene item lock changed"
    }

    fn summary(&self) -> &str {
        "Fires when a scene item's lock state is toggled in OBS."
    }

    fn search_text(&self) -> &str {
        "obs source scene item lock locked unlock"
    }

    fn icon_name(&self) -> &str {
        "lock"
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
        messages::CONDITION_ANY_SCENE_ITEM_LOCK_CHANGE.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::SOURCE_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::SOURCE_LOCK_CHANGED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(scene) = event
            .payload
            .get(fields::SCENE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::scene::NAME.to_owned(),
                Variant::String(scene.to_owned()),
            );
        }
        if let Some(source) = event
            .payload
            .get(fields::SOURCE_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::source::NAME.to_owned(),
                Variant::String(source.to_owned()),
            );
        }
        if let Some(locked) = event
            .payload
            .get(fields::IS_LOCKED)
            .and_then(|v| v.as_bool())
        {
            stack = stack.set(
                variables::source::IS_LOCKED.to_owned(),
                Variant::Bool(locked),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: variables::scene::NAME.to_owned(),
                    kind: VariantKind::String,
                    label: "Scene name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::source::NAME.to_owned(),
                    kind: VariantKind::String,
                    label: "Source name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::source::IS_LOCKED.to_owned(),
                    kind: VariantKind::Bool,
                    label: "Source locked".to_owned(),
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
    use forge_types::TriggerConfig;
    use serde_json::json;

    fn lock_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Obs, "obs.source.lock_changed", payload)
    }

    #[test]
    fn matches_trigger_fires_on_exact_lock_kind() {
        let cfg = TriggerConfig::new();
        let event = lock_event(json!({}));
        assert!(SourceSceneItemLockChangedDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn matches_trigger_rejects_sibling_source_kinds() {
        let cfg = TriggerConfig::new();
        for sibling in ["obs.source.visibility_changed", "obs.source.input_created"] {
            let event = Event::new(EventSource::Obs, sibling, json!({}));
            assert!(
                !SourceSceneItemLockChangedDescriptor.matches_trigger(&cfg, &event),
                "must not fire on sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn matches_trigger_rejects_non_source_kind() {
        let cfg = TriggerConfig::new();
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!SourceSceneItemLockChangedDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn build_arg_stack_extracts_scene_source_and_locked_flag() {
        let event =
            lock_event(json!({ "scene_name": "Main", "source_name": "Cam", "is_locked": true }));
        let stack = SourceSceneItemLockChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Main".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Cam".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.is_locked"),
            Some(&Variant::Bool(true)),
        );
    }

    #[test]
    fn build_arg_stack_preserves_false_locked_flag() {
        let event =
            lock_event(json!({ "scene_name": "Main", "source_name": "Cam", "is_locked": false }));
        let stack = SourceSceneItemLockChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.is_locked"),
            Some(&Variant::Bool(false)),
        );
    }

    #[test]
    fn build_arg_stack_omits_keys_when_payload_fields_absent() {
        let stack = SourceSceneItemLockChangedDescriptor.build_arg_stack(&lock_event(json!({})));
        assert!(stack.get("obs.scene.name").is_none());
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.is_locked").is_none());
    }
}
