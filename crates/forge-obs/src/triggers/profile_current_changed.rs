use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::profile as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text(
    variables::profile::NAME,
    fields::PROFILE_NAME,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const PROFILE_CURRENT_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::PROFILE_CURRENT_CHANGED,
    source: EventSource::Obs,
    label: "OBS profile changed",
    summary: "Fires when OBS finishes switching to a different profile.",
    search_text: "obs profile changed switched current",
    icon: "settings",
    prefix: event_kinds::PROFILE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::PROFILE_CURRENT_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_PROFILE),
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
    fn matches_current_changed_and_rejects_list_changed_under_profile_prefix() {
        let d = PROFILE_CURRENT_CHANGED;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.profile.current_changed", json!({})),
        ));
        assert!(!d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.profile.list_changed", json!({})),
        ));
    }

    #[test]
    fn arg_stack_binds_profile_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "obs.profile.current_changed",
            json!({ "profile_name": "Streaming" }),
        );
        let stack = PROFILE_CURRENT_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.profile.name"),
            Some(&Variant::String("Streaming".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.profile.current_changed", json!({}));
        let stack = PROFILE_CURRENT_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.profile.name").is_none());
    }
}
