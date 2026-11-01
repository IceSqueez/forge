use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::hotkey as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(variables::hotkey::NAME, "Hotkey name", fields::HOTKEY_NAME),
    EventVariable::text(variables::hotkey::ID, "Hotkey ID", fields::HOTKEY_ID),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const HOTKEY_TRIGGERED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::HOTKEY_TRIGGERED,
    source: EventSource::VTube,
    label: "VTube Studio hotkey triggered",
    summary: "Fires when a VTube Studio hotkey is activated.",
    search_text: "vtube hotkey triggered activated shortcut",
    icon: "zap",
    prefix: event_kinds::HOTKEY_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::HOTKEY_TRIGGERED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_HOTKEY),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::Event;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::{TriggerConfig, Variant};
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::VTube, kind, payload)
    }

    #[test]
    fn matches_only_the_exact_triggered_kind() {
        let d = HOTKEY_TRIGGERED;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.hotkey.triggered", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = HOTKEY_TRIGGERED;
        let stack = d.build_arg_stack(&event(
            "vtube.hotkey.triggered",
            json!({ "hotkey_name": "Wave", "hotkey_id": "hk-7" }),
        ));
        assert_eq!(
            stack.get("vtube.hotkey.name"),
            Some(&Variant::String("Wave".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.hotkey.id"),
            Some(&Variant::String("hk-7".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = HOTKEY_TRIGGERED;
        let stack = d.build_arg_stack(&event(
            "vtube.hotkey.triggered",
            json!({ "hotkey_id": "hk-7" }),
        ));
        assert!(stack.get("vtube.hotkey.name").is_none());
        assert_eq!(
            stack.get("vtube.hotkey.id"),
            Some(&Variant::String("hk-7".to_owned()))
        );
    }
}
