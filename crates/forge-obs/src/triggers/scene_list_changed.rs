use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::scene as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text_list(
    variables::scene::ALL_NAMES,
    "All scene names",
    fields::ALL_NAMES,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SCENE_LIST_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SCENE_LIST_CHANGED,
    source: EventSource::Obs,
    label: "OBS scene list changed",
    summary: "Fires when scenes are added, removed, or reordered in OBS.",
    search_text: "obs scene list changed added removed reordered",
    icon: "layout-grid",
    prefix: event_kinds::SCENE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SCENE_LIST_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_LIST_CHANGE),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn matches_list_changed_kind() {
        let d = SCENE_LIST_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.list_changed",
            json!({ "all_names": ["Menu", "Gameplay"] }),
        );
        assert!(d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn does_not_match_other_scene_kind() {
        let d = SCENE_LIST_CHANGED;
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_collects_all_names_into_string_array() {
        let d = SCENE_LIST_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.list_changed",
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
        let d = SCENE_LIST_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.list_changed",
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
        let d = SCENE_LIST_CHANGED;
        let event = Event::new(EventSource::Obs, "obs.scene.list_changed", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.scene_names"), None);
    }
}
