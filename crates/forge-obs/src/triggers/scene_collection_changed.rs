use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::collection as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text(
    variables::collection::NAME,
    fields::COLLECTION_NAME,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SCENE_COLLECTION_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::COLLECTION_CHANGED,
    source: EventSource::Obs,
    label: "OBS scene collection changed",
    summary: "Fires after OBS finishes switching to a different scene collection.",
    search_text: "obs scene collection changed switched",
    icon: "stack-2",
    prefix: event_kinds::COLLECTION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::COLLECTION_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_COLLECTION),
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
    fn matches_collection_changed_kind() {
        let d = SCENE_COLLECTION_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.changed",
            json!({ "collection_name": "Stream" }),
        );
        assert!(d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn does_not_match_collection_changing_kind() {
        let d = SCENE_COLLECTION_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.changing",
            json!({ "collection_name": "Stream" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_maps_name_to_collection() {
        let d = SCENE_COLLECTION_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.changed",
            json!({ "collection_name": "Stream" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.collection"),
            Some(&Variant::String("Stream".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_collection_when_name_missing() {
        let d = SCENE_COLLECTION_CHANGED;
        let event = Event::new(EventSource::Obs, "obs.collection.changed", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.collection"), None);
    }
}
