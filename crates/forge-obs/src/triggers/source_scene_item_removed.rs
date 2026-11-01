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
    EventVariable::text(variables::scene::NAME, "Scene name", fields::SCENE_NAME),
    EventVariable::text(variables::source::NAME, "Source name", fields::SOURCE_NAME),
    EventVariable::int(variables::source::ITEM_ID, "Scene item ID", fields::ITEM_ID),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SOURCE_SCENE_ITEM_REMOVED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SOURCE_SCENE_ITEM_REMOVED,
    source: EventSource::Obs,
    label: "OBS scene item removed",
    summary: "Fires when a source is removed from a scene in OBS.",
    search_text: "obs scene item source removed deleted",
    icon: "minus-circle",
    prefix: event_kinds::SOURCE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SOURCE_SCENE_ITEM_REMOVED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_ITEM_REMOVED),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::{TriggerConfig, Variant};
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::Obs, kind, payload)
    }

    #[test]
    fn fires_on_the_scene_item_removed_kind_only() {
        let cfg = TriggerConfig::new();
        for (kind, expected) in [
            ("obs.source.scene_item_removed", true),
            ("obs.source.scene_item_created", false),
            ("obs.source.input_removed", false),
            ("obs.scene.removed", false),
        ] {
            assert_eq!(
                SOURCE_SCENE_ITEM_REMOVED.matches_trigger(&cfg, &event(kind, json!({}))),
                expected,
                "{kind}"
            );
        }
    }

    #[test]
    fn arg_stack_carries_the_scene_source_and_item_id() {
        let stack = SOURCE_SCENE_ITEM_REMOVED.build_arg_stack(&event(
            "obs.source.scene_item_removed",
            json!({ "scene_name": "Gameplay", "source_name": "Webcam", "item_id": 42 }),
        ));

        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Gameplay".to_owned()))
        );
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Webcam".to_owned()))
        );
        assert_eq!(stack.get("obs.source.item_id"), Some(&Variant::Int(42)));
    }

    #[test]
    fn arg_stack_omits_every_key_whose_payload_field_is_absent_or_mistyped() {
        let stack = SOURCE_SCENE_ITEM_REMOVED.build_arg_stack(&event(
            "obs.source.scene_item_removed",
            json!({ "scene_name": 7, "item_id": "42" }),
        ));

        for key in ["obs.scene.name", "obs.source.name", "obs.source.item_id"] {
            assert!(stack.get(key).is_none(), "{key}");
        }
    }
}
