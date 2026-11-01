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
    EventVariable::text(variables::source::NAME, "Source name", fields::SOURCE_NAME),
    EventVariable::text(variables::source::KIND, "Source kind", fields::SOURCE_KIND),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SOURCE_INPUT_CREATED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SOURCE_INPUT_CREATED,
    source: EventSource::Obs,
    label: "OBS input source created",
    summary: "Fires when a new input source is added to OBS.",
    search_text: "obs input source created added new",
    icon: "plus-circle",
    prefix: event_kinds::SOURCE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SOURCE_INPUT_CREATED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_INPUT_CREATED),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::super::source_input_created::SOURCE_INPUT_CREATED;
    use super::super::source_input_removed::SOURCE_INPUT_REMOVED;
    use super::super::source_input_renamed::SOURCE_INPUT_RENAMED;
    use super::super::source_scene_item_visibility_changed::SOURCE_SCENE_ITEM_VISIBILITY_CHANGED;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ALL_SOURCE_KINDS: [&str; 4] = [
        "obs.source.input_created",
        "obs.source.input_removed",
        "obs.source.input_renamed",
        "obs.source.visibility_changed",
    ];

    #[test]
    fn each_source_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 4] = [
            ("obs.source.input_created", &SOURCE_INPUT_CREATED),
            ("obs.source.input_removed", &SOURCE_INPUT_REMOVED),
            ("obs.source.input_renamed", &SOURCE_INPUT_RENAMED),
            (
                "obs.source.visibility_changed",
                &SOURCE_SCENE_ITEM_VISIBILITY_CHANGED,
            ),
        ];
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_SOURCE_KINDS {
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
    fn source_descriptor_rejects_non_source_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!SOURCE_INPUT_CREATED.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn input_created_arg_stack_extracts_name_and_kind() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.input_created",
            json!({ "source_name": "Mic", "source_kind": "wasapi_input_capture" }),
        );
        let stack = SOURCE_INPUT_CREATED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.kind"),
            Some(&Variant::String("wasapi_input_capture".to_owned())),
        );
    }

    #[test]
    fn input_created_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "obs.source.input_created", json!({}));
        let stack = SOURCE_INPUT_CREATED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.kind").is_none());
    }
}
