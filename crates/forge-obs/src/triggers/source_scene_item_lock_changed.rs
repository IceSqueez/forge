use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::source as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(variables::scene::NAME, "Scene name", fields::SCENE_NAME),
    EventVariable::text(variables::source::NAME, "Source name", fields::SOURCE_NAME),
    EventVariable::flag(
        variables::source::IS_LOCKED,
        "Source locked",
        fields::IS_LOCKED,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SOURCE_SCENE_ITEM_LOCK_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SOURCE_LOCK_CHANGED,
    source: EventSource::Obs,
    label: "OBS scene item lock changed",
    summary: "Fires when a scene item's lock state is toggled in OBS.",
    search_text: "obs source scene item lock locked unlock",
    icon: "lock",
    prefix: event_kinds::SOURCE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SOURCE_LOCK_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_ITEM_LOCK_CHANGE),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::TriggerConfig;
    use forge_types::Variant;
    use serde_json::json;

    fn lock_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Obs, "obs.source.lock_changed", payload)
    }

    #[test]
    fn matches_trigger_fires_on_exact_lock_kind() {
        let cfg = TriggerConfig::new();
        let event = lock_event(json!({}));
        assert!(SOURCE_SCENE_ITEM_LOCK_CHANGED.matches_trigger(&cfg, &event));
    }

    #[test]
    fn matches_trigger_rejects_sibling_source_kinds() {
        let cfg = TriggerConfig::new();
        for sibling in ["obs.source.visibility_changed", "obs.source.input_created"] {
            let event = Event::new(EventSource::Obs, sibling, json!({}));
            assert!(
                !SOURCE_SCENE_ITEM_LOCK_CHANGED.matches_trigger(&cfg, &event),
                "must not fire on sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn matches_trigger_rejects_non_source_kind() {
        let cfg = TriggerConfig::new();
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!SOURCE_SCENE_ITEM_LOCK_CHANGED.matches_trigger(&cfg, &event));
    }

    #[test]
    fn build_arg_stack_extracts_scene_source_and_locked_flag() {
        let event =
            lock_event(json!({ "scene_name": "Main", "source_name": "Cam", "is_locked": true }));
        let stack = SOURCE_SCENE_ITEM_LOCK_CHANGED.build_arg_stack(&event);
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
        let stack = SOURCE_SCENE_ITEM_LOCK_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.is_locked"),
            Some(&Variant::Bool(false)),
        );
    }

    #[test]
    fn build_arg_stack_omits_keys_when_payload_fields_absent() {
        let stack = SOURCE_SCENE_ITEM_LOCK_CHANGED.build_arg_stack(&lock_event(json!({})));
        assert!(stack.get("obs.scene.name").is_none());
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.is_locked").is_none());
    }
}
