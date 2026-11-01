use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const FILTER_CREATED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::FILTER_CREATED,
    source: EventSource::Obs,
    label: "OBS filter created",
    summary: "Fires when a filter is added to an OBS source.",
    search_text: "obs filter created added source",
    icon: "filter",
    prefix: event_kinds::FILTER_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::FILTER_CREATED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_FILTER_CREATED),
    config_fields: no_config_fields,
    variables: families::filter_created_variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;

    #[test]
    fn arg_stack_extracts_filter_kind() {
        let event = Event::new(
            EventSource::Obs,
            "obs.filter.created",
            json!({ "filter_kind": "noise_suppress_filter" }),
        );
        assert_eq!(
            FILTER_CREATED
                .build_arg_stack(&event)
                .get("obs.filter.kind"),
            Some(&Variant::String("noise_suppress_filter".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_filter_kind_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.filter.created", json!({}));
        assert!(
            FILTER_CREATED
                .build_arg_stack(&event)
                .get("obs.filter.kind")
                .is_none()
        );
    }
}
