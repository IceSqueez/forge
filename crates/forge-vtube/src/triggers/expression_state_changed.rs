use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::expression as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(
        variables::expression::FILE,
        "Expression file",
        fields::EXPRESSION_FILE,
    ),
    EventVariable::flag(
        variables::expression::IS_ACTIVE,
        "Expression active",
        fields::IS_ACTIVE,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const EXPRESSION_STATE_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::EXPRESSION_STATE_CHANGED,
    source: EventSource::VTube,
    label: "VTube Studio expression state changed",
    summary: "Fires when a VTube Studio expression is activated or deactivated.",
    search_text: "vtube expression state changed activated deactivated",
    icon: "smile",
    prefix: event_kinds::EXPRESSION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::EXPRESSION_STATE_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_EXPRESSION),
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
    fn matches_only_the_exact_state_changed_kind() {
        let d = EXPRESSION_STATE_CHANGED;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.expression.state_changed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.expression.other", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_file_and_is_active_distinguishing_false_from_absent() {
        let d = EXPRESSION_STATE_CHANGED;
        for (payload, file, active) in [
            (
                json!({ "expression_file": "Smile.exp3.json", "is_active": true }),
                Some("Smile.exp3.json"),
                Some(true),
            ),
            (json!({ "is_active": false }), None, Some(false)),
            (json!({}), None, None),
        ] {
            let stack = d.build_arg_stack(&event("vtube.expression.state_changed", payload));
            assert_eq!(
                stack.get("vtube.expression.file"),
                file.map(|f| Variant::String(f.to_owned())).as_ref()
            );
            assert_eq!(
                stack.get("vtube.expression.is_active"),
                active.map(Variant::Bool).as_ref()
            );
        }
    }
}
