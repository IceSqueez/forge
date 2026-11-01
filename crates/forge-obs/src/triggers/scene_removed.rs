use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::scene as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text(
    variables::scene::NAME,
    "Scene name",
    fields::SCENE_NAME,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SCENE_REMOVED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SCENE_REMOVED,
    source: EventSource::Obs,
    label: "OBS scene removed",
    summary: "Fires when a scene is deleted in OBS.",
    search_text: "obs scene removed deleted",
    icon: "layout-grid",
    prefix: event_kinds::SCENE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SCENE_REMOVED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_REMOVED),
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
    fn matches_only_scene_removed_kind_within_scene_family() {
        let d = SCENE_REMOVED;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.scene.removed", json!({})),
        ));
        for sibling in [
            "obs.scene.created",
            "obs.scene.renamed",
            "obs.scene.changed",
            "obs.scene.preview_changed",
            "obs.scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.scene.removed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_scene_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.removed",
            json!({ "scene_name": "Intro" }),
        );
        let stack = SCENE_REMOVED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("Intro".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.scene.removed", json!({}));
        let stack = SCENE_REMOVED.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
    }
}
