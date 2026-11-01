use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::collection as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text_list(
    variables::collection::ALL_NAMES,
    fields::ALL_NAMES,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const COLLECTION_LIST_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::COLLECTION_LIST_CHANGED,
    source: EventSource::Obs,
    label: "OBS scene collection list changed",
    summary: "Fires when scene collections are added or removed in OBS.",
    search_text: "obs scene collection list changed added removed",
    icon: "stack-2",
    prefix: event_kinds::COLLECTION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::COLLECTION_LIST_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_COLLECTION_LIST_CHANGE),
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
    fn matches_list_changed_and_rejects_sibling_collection_kinds() {
        let d = COLLECTION_LIST_CHANGED;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.collection.list_changed", json!({})),
        ));
        for sibling in ["obs.collection.changing", "obs.collection.changed"] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.collection.list_changed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_collects_all_names_into_string_array() {
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.list_changed",
            json!({ "all_names": ["Main", "Alt"] }),
        );
        let stack = COLLECTION_LIST_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.collections.names"),
            Some(&Variant::Array(vec![
                Variant::String("Main".to_owned()),
                Variant::String("Alt".to_owned()),
            ])),
        );
    }

    #[test]
    fn arg_stack_skips_non_string_array_elements() {
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.list_changed",
            json!({ "all_names": ["Main", 7, null, "Alt"] }),
        );
        let stack = COLLECTION_LIST_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.collections.names"),
            Some(&Variant::Array(vec![
                Variant::String("Main".to_owned()),
                Variant::String("Alt".to_owned()),
            ])),
        );
    }

    #[test]
    fn arg_stack_omits_key_when_all_names_absent() {
        let event = Event::new(EventSource::Obs, "obs.collection.list_changed", json!({}));
        let stack = COLLECTION_LIST_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.collections.names").is_none());
    }
}
