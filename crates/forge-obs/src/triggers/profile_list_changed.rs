use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::profile as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text_list(
    variables::profile::ALL_NAMES,
    fields::ALL_NAMES,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const PROFILE_LIST_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::PROFILE_LIST_CHANGED,
    source: EventSource::Obs,
    label: "OBS profile list changed",
    summary: "Fires when profiles are added or removed in OBS.",
    search_text: "obs profile list changed added removed",
    icon: "settings",
    prefix: event_kinds::PROFILE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::PROFILE_LIST_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_PROFILE_LIST_CHANGE),
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
    fn matches_list_changed_and_rejects_current_changed_under_profile_prefix() {
        let d = PROFILE_LIST_CHANGED;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.profile.list_changed", json!({})),
        ));
        assert!(!d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.profile.current_changed", json!({})),
        ));
    }

    #[test]
    fn arg_stack_collects_string_names_and_skips_other_elements() {
        for (label, input) in [
            (
                "arg_stack_collects_all_names_into_string_array",
                json!(["Streaming", "Recording"]),
            ),
            (
                "arg_stack_skips_non_string_array_elements",
                json!(["Streaming", 42, true, "Recording"]),
            ),
        ] {
            let event = Event::new(
                EventSource::Obs,
                "obs.profile.list_changed",
                json!({ "all_names": input }),
            );
            let stack = PROFILE_LIST_CHANGED.build_arg_stack(&event);
            assert_eq!(
                stack.get("obs.profiles.names"),
                Some(&Variant::Array(vec![
                    Variant::String("Streaming".to_owned()),
                    Variant::String("Recording".to_owned()),
                ])),
                "{label}"
            );
        }
    }

    #[test]
    fn arg_stack_omits_key_when_all_names_absent() {
        let event = Event::new(EventSource::Obs, "obs.profile.list_changed", json!({}));
        let stack = PROFILE_LIST_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.profiles.names").is_none());
    }
}
