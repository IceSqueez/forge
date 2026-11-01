use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const FILTER_ENABLED_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::FILTER_ENABLED_CHANGED,
    source: EventSource::Obs,
    label: "OBS filter enable state changed",
    summary: "Fires when a filter on an OBS source is enabled or disabled.",
    search_text: "obs filter enabled disabled toggled source",
    icon: "filter",
    prefix: event_kinds::FILTER_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::FILTER_ENABLED_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_FILTER_ENABLE_STATE_CHANGED),
    config_fields: no_config_fields,
    variables: families::filter_enabled_changed_variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;

    #[test]
    fn arg_stack_preserves_is_enabled_both_true_and_false() {
        for flag in [true, false] {
            let event = Event::new(
                EventSource::Obs,
                "obs.filter.enabled_changed",
                json!({ "is_enabled": flag }),
            );
            assert_eq!(
                FILTER_ENABLED_CHANGED
                    .build_arg_stack(&event)
                    .get("obs.filter.is_enabled"),
                Some(&Variant::Bool(flag)),
            );
        }
    }

    #[test]
    fn arg_stack_omits_is_enabled_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.filter.enabled_changed", json!({}));
        assert!(
            FILTER_ENABLED_CHANGED
                .build_arg_stack(&event)
                .get("obs.filter.is_enabled")
                .is_none()
        );
    }
}
