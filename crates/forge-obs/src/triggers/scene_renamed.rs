use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::scene as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(
        variables::scene::NAME_OLD,
        "Old scene name",
        fields::SCENE_NAME_OLD,
    ),
    EventVariable::text(
        variables::scene::NAME_NEW,
        "New scene name",
        fields::SCENE_NAME_NEW,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SCENE_RENAMED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SCENE_RENAMED,
    source: EventSource::Obs,
    label: "OBS scene renamed",
    summary: "Fires when a scene is renamed in OBS.",
    search_text: "obs scene renamed name changed",
    icon: "layout-grid",
    prefix: event_kinds::SCENE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SCENE_RENAMED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_SCENE_RENAMED),
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
    fn matches_only_scene_renamed_kind_within_scene_family() {
        let d = SCENE_RENAMED;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.scene.renamed", json!({})),
        ));
        for sibling in [
            "obs.scene.created",
            "obs.scene.removed",
            "obs.scene.changed",
            "obs.scene.preview_changed",
            "obs.scene.list_changed",
        ] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.scene.renamed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_binds_old_and_new_names_to_distinct_keys() {
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.renamed",
            json!({ "scene_name_old": "Old", "scene_name_new": "New" }),
        );
        let stack = SCENE_RENAMED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene.name_old"),
            Some(&Variant::String("Old".to_owned())),
        );
        assert_eq!(
            stack.get("obs.scene.name_new"),
            Some(&Variant::String("New".to_owned())),
        );
    }

    #[test]
    fn arg_stack_binds_only_present_name_field() {
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.renamed",
            json!({ "scene_name_new": "New" }),
        );
        let stack = SCENE_RENAMED.build_arg_stack(&event);
        assert!(stack.get("obs.scene.name_old").is_none());
        assert_eq!(
            stack.get("obs.scene.name_new"),
            Some(&Variant::String("New".to_owned())),
        );
    }
}
