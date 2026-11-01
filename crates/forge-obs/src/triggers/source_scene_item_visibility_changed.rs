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
    EventVariable::text(variables::scene::NAME, fields::SCENE_NAME),
    EventVariable::text(variables::source::NAME, fields::SOURCE_NAME),
    EventVariable::flag(variables::source::IS_VISIBLE, fields::IS_VISIBLE),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SOURCE_SCENE_ITEM_VISIBILITY_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SOURCE_VISIBILITY_CHANGED,
    source: EventSource::Obs,
    label: "OBS scene item visibility changed",
    summary: "Fires when a scene item's visibility is toggled in OBS.",
    search_text: "obs source scene item visible hidden show hide visibility",
    icon: "eye",
    prefix: event_kinds::SOURCE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SOURCE_VISIBILITY_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_ITEM_VISIBILITY_CHANGE),
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

    #[test]
    fn visibility_arg_stack_extracts_scene_source_and_enabled_flag() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.visibility_changed",
            json!({ "scene_name": "Main", "source_name": "Cam", "is_visible": true }),
        );
        let stack = SOURCE_SCENE_ITEM_VISIBILITY_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Main".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Cam".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.is_visible"),
            Some(&Variant::Bool(true)),
        );
    }

    #[test]
    fn visibility_arg_stack_preserves_false_enabled_flag() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.visibility_changed",
            json!({ "scene_name": "Main", "source_name": "Cam", "is_visible": false }),
        );
        let stack = SOURCE_SCENE_ITEM_VISIBILITY_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.is_visible"),
            Some(&Variant::Bool(false)),
        );
    }

    #[test]
    fn visibility_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "obs.source.visibility_changed", json!({}));
        let stack = SOURCE_SCENE_ITEM_VISIBILITY_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.is_visible").is_none());
    }
}
