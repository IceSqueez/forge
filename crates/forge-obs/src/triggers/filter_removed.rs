use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const FILTER_REMOVED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::FILTER_REMOVED,
    source: EventSource::Obs,
    label: "OBS filter removed",
    summary: "Fires when a filter is removed from an OBS source.",
    search_text: "obs filter removed deleted source",
    icon: "filter-x",
    prefix: event_kinds::FILTER_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::FILTER_REMOVED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_FILTER_REMOVED),
    config_fields: no_config_fields,
    variables: families::filter_source_variables,
};

#[cfg(test)]
mod tests {
    use super::super::filter_created::FILTER_CREATED;
    use super::super::filter_enabled_changed::FILTER_ENABLED_CHANGED;
    use super::super::filter_removed::FILTER_REMOVED;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ALL_FILTER_KINDS: [&str; 3] = [
        "obs.filter.created",
        "obs.filter.removed",
        "obs.filter.enabled_changed",
    ];

    #[test]
    fn each_filter_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 3] = [
            ("obs.filter.created", &FILTER_CREATED),
            ("obs.filter.removed", &FILTER_REMOVED),
            ("obs.filter.enabled_changed", &FILTER_ENABLED_CHANGED),
        ];
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_FILTER_KINDS {
                let event = Event::new(EventSource::Obs, kind, json!({}));
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &event),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn filter_descriptors_reject_non_filter_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [&dyn TriggerKindDescriptor; 3] =
            [&FILTER_CREATED, &FILTER_REMOVED, &FILTER_ENABLED_CHANGED];
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        for descriptor in descriptors {
            assert!(!descriptor.matches_trigger(&cfg, &event));
        }
    }

    #[test]
    fn filter_source_arg_stack_extracts_source_and_filter_names() {
        let event = Event::new(
            EventSource::Obs,
            "obs.filter.removed",
            json!({ "source_name": "Mic", "filter_name": "Noise Gate" }),
        );
        let stack = FILTER_REMOVED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.filter.name"),
            Some(&Variant::String("Noise Gate".to_owned())),
        );
    }

    #[test]
    fn filter_source_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "obs.filter.removed", json!({}));
        let stack = FILTER_REMOVED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.filter.name").is_none());
    }
}
