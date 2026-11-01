use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const MODEL_LOADED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::MODEL_LOADED,
    source: EventSource::VTube,
    label: "VTube Studio model loaded",
    summary: "Fires when a VTube Studio model is loaded.",
    search_text: "vtube model loaded avatar switch",
    icon: "user",
    prefix: event_kinds::MODEL_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::MODEL_LOADED),
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
    fn matches_only_the_exact_loaded_kind() {
        let d = MODEL_LOADED;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.unloaded", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = MODEL_LOADED;
        let stack = d.build_arg_stack(&event(
            "vtube.model.loaded",
            json!({ "model_name": "Aria", "model_id": "m-42" }),
        ));
        assert_eq!(
            stack.get("vtube.model.name"),
            Some(&Variant::String("Aria".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.model.id"),
            Some(&Variant::String("m-42".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = MODEL_LOADED;
        let stack = d.build_arg_stack(&event(
            "vtube.model.loaded",
            json!({ "model_name": "Aria" }),
        ));
        assert_eq!(
            stack.get("vtube.model.name"),
            Some(&Variant::String("Aria".to_owned()))
        );
        assert!(stack.get("vtube.model.id").is_none());
    }
}
