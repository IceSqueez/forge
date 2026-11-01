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

pub(crate) const SCENE_CREATED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SCENE_CREATED,
    source: EventSource::Obs,
    label: "OBS scene created",
    summary: "Fires when a new scene is created in OBS.",
    search_text: "obs scene created added new",
    icon: "layout-grid",
    prefix: event_kinds::SCENE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SCENE_CREATED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_CREATED),
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
    fn matches_only_scene_created_kind_within_scene_family() {
        let d = SCENE_CREATED;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.scene.created", json!({})),
        ));
        for sibling in [
            "obs.scene.removed",
            "obs.scene.renamed",
            "obs.scene.changed",
            "obs.scene.preview_changed",
            "obs.scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.scene.created wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_scene_name_as_string() {
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.created",
            json!({ "scene_name": "BRB" }),
        );
        let stack = SCENE_CREATED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name"),
            Some(&Variant::String("BRB".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.scene.created", json!({}));
        let stack = SCENE_CREATED.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name").is_none());
    }
}
