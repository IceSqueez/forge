use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const ITEM_ADDED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::ITEM_ADDED,
    source: EventSource::VTube,
    label: "VTube Studio item added",
    summary: "Fires when an item is loaded into the VTube Studio scene.",
    search_text: "vtube item added loaded prop sticker",
    icon: "image",
    prefix: event_kinds::ITEM_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::ITEM_ADDED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_ITEM),
    config_fields: no_config_fields,
    variables: families::item_variables,
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
    fn matches_only_the_exact_added_kind() {
        let d = ITEM_ADDED;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.item.added", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.item.removed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = ITEM_ADDED;
        let stack = d.build_arg_stack(&event(
            "vtube.item.added",
            json!({ "item_instance_id": "inst-7", "item_file_name": "hat.png" }),
        ));
        assert_eq!(
            stack.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-7".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.item.file_name"),
            Some(&Variant::String("hat.png".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = ITEM_ADDED;
        let stack = d.build_arg_stack(&event(
            "vtube.item.added",
            json!({ "item_instance_id": "inst-7" }),
        ));
        assert_eq!(
            stack.get("vtube.item.instance_id"),
            Some(&Variant::String("inst-7".to_owned()))
        );
        assert!(stack.get("vtube.item.file_name").is_none());
    }
}
