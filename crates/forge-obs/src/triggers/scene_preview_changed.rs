use forge_events::Event;
use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
};
use forge_registry::{FormField, SubActionConfigExt};
use forge_types::{TriggerConfig, UserText};

use crate::vocabulary::fields::scene as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, picker_keys, trigger_ids, variables};

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    if event.kind != event_kinds::SCENE_PREVIEW_CHANGED {
        return false;
    }
    match config.str_nonempty("scene") {
        Some(s) => {
            event
                .payload
                .get(fields::SCENE_NAME_NEW)
                .and_then(|v| v.as_str())
                == Some(s)
        }
        None => true,
    }
}

fn condition_display(config: &TriggerConfig) -> UserText {
    match config.str_nonempty("scene") {
        Some(s) => messages::condition_preview(s).into(),
        None => messages::CONDITION_ANY_PREVIEW_SCENE.into(),
    }
}

fn config_fields() -> Vec<FormField> {
    vec![FormField::Optional {
        key: "scene",
        label: "Scene name (leave empty to match any)",
        inner: Box::new(FormField::DynamicSelect {
            key: "scene",
            label: "Scene",
            options_key: picker_keys::SCENE_NAMES,
        }),
    }]
}

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(
        variables::scene::PROGRAM,
        "Scene name",
        fields::SCENE_NAME_NEW,
    ),
    EventVariable::text(
        variables::scene::PREVIOUS,
        "Previous scene name",
        fields::SCENE_NAME_OLD,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SCENE_PREVIEW_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SCENE_PREVIEW_CHANGED,
    source: EventSource::Obs,
    label: "OBS preview scene changed",
    summary: "Fires when the Studio Mode preview scene changes.",
    search_text: "obs preview scene changed studio mode",
    icon: "layers-subtract",
    prefix: event_kinds::SCENE_PREFIX,
    matcher: TriggerMatcher::Custom(matches),
    condition: TriggerCondition::FromConfig(condition_display),
    config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn preview_event(old: &str, new: &str) -> Event {
        Event::new(
            EventSource::Obs,
            "obs.scene.preview_changed",
            json!({ "scene_name_old": old, "scene_name_new": new }),
        )
    }

    fn named_config(scene: &str) -> TriggerConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(scene.to_owned()))])
    }

    #[test]
    fn matches_preview_changed_kind_with_empty_config() {
        let d = SCENE_PREVIEW_CHANGED;
        assert!(d.matches_trigger(&BTreeMap::new(), &preview_event("Menu", "Gameplay")));
    }

    #[test]
    fn matches_when_config_scene_equals_new_preview() {
        let d = SCENE_PREVIEW_CHANGED;
        assert!(d.matches_trigger(
            &named_config("Gameplay"),
            &preview_event("Menu", "Gameplay")
        ));
    }

    #[test]
    fn does_not_match_when_config_scene_differs_from_new_preview() {
        let d = SCENE_PREVIEW_CHANGED;
        assert!(!d.matches_trigger(&named_config("BRB"), &preview_event("Menu", "Gameplay")));
    }

    #[test]
    fn does_not_match_program_scene_changed_kind() {
        let d = SCENE_PREVIEW_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.changed",
            json!({ "scene_name_new": "Gameplay" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_maps_new_to_scene_and_old_to_previous() {
        let d = SCENE_PREVIEW_CHANGED;
        let stack = d.build_arg_stack(&preview_event("Menu", "Gameplay"));
        assert_eq!(
            stack.get("obs.scene"),
            Some(&Variant::String("Gameplay".to_owned()))
        );
        assert_eq!(
            stack.get("obs.previous_scene"),
            Some(&Variant::String("Menu".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_previous_scene_when_name_old_missing() {
        let d = SCENE_PREVIEW_CHANGED;
        let event = Event::new(
            EventSource::Obs,
            "obs.scene.preview_changed",
            json!({ "scene_name_new": "Gameplay" }),
        );
        let stack = d.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.scene"),
            Some(&Variant::String("Gameplay".to_owned()))
        );
        assert_eq!(stack.get("obs.previous_scene"), None);
    }
}
