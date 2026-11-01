use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const MODEL_CONFIG_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::MODEL_CONFIG_CHANGED,
    source: EventSource::VTube,
    label: "VTube Studio model config changed",
    summary: "Fires when the configuration of the active VTube Studio model changes.",
    search_text: "vtube model config changed settings avatar",
    icon: "settings",
    prefix: event_kinds::MODEL_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::MODEL_CONFIG_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_MODEL),
    config_fields: no_config_fields,
    variables: families::model_variables,
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
    fn matches_only_the_exact_config_changed_kind() {
        let d = MODEL_CONFIG_CHANGED;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.model.config_changed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_model_id_and_name() {
        let d = MODEL_CONFIG_CHANGED;
        for (payload, name, id) in [
            (
                json!({ "model_name": "Aria", "model_id": "m-42" }),
                Some("Aria"),
                Some("m-42"),
            ),
            (json!({ "model_name": "Aria" }), Some("Aria"), None),
            (json!({}), None, None),
        ] {
            let stack = d.build_arg_stack(&event("vtube.model.config_changed", payload));
            assert_eq!(
                stack.get("vtube.model.name"),
                name.map(|s| Variant::String(s.to_owned())).as_ref()
            );
            assert_eq!(
                stack.get("vtube.model.id"),
                id.map(|s| Variant::String(s.to_owned())).as_ref()
            );
        }
    }
}
