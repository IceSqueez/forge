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
    if event.kind != event_kinds::SCENE_CHANGED {
        return false;
    }
    match config.str_nonempty("scene") {
        Some(s) => event.payload.get(fields::TO_SCENE).and_then(|v| v.as_str()) == Some(s),
        None => true,
    }
}

fn condition_display(config: &TriggerConfig) -> UserText {
    match config.str_nonempty("scene") {
        Some(s) => messages::condition_scene(s).into(),
        None => messages::CONDITION_ANY_SCENE.into(),
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
    EventVariable::text(variables::scene::PROGRAM, "Scene name", fields::TO_SCENE),
    EventVariable::text(
        variables::scene::PREVIOUS,
        "Previous scene name",
        fields::FROM_SCENE,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SCENE_CURRENT_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SCENE_CHANGED,
    source: EventSource::Obs,
    label: "OBS scene changed",
    summary: "Fires when the current OBS program scene changes.",
    search_text: "obs scene changed current program switch",
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

    fn named_config(scene: &str) -> BTreeMap<String, Variant> {
        BTreeMap::from([("scene".to_owned(), Variant::String(scene.to_owned()))])
    }

    fn scene_changed_event(from: &str, to: &str) -> Event {
        Event::new(
            EventSource::Obs,
            "obs.scene.changed",
            json!({ "from_scene": from, "to_scene": to }),
        )
    }

    #[test]
    fn matches_any_scene_when_config_empty() {
        let d = SCENE_CURRENT_CHANGED;
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn matches_exact_scene_name() {
        let d = SCENE_CURRENT_CHANGED;
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(d.matches_trigger(&named_config("Gameplay"), &event));
    }

    #[test]
    fn does_not_match_different_scene_name() {
        let d = SCENE_CURRENT_CHANGED;
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(!d.matches_trigger(&named_config("BRB"), &event));
    }

    #[test]
    fn empty_string_scene_config_matches_any() {
        let d = SCENE_CURRENT_CHANGED;
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(d.matches_trigger(&named_config(""), &event));
    }

    #[test]
    fn does_not_match_non_scene_changed_event() {
        let d = SCENE_CURRENT_CHANGED;
        let event = Event::new(EventSource::Obs, "obs.scene.created", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_populates_scene_keys() {
        let d = SCENE_CURRENT_CHANGED;
        let event = scene_changed_event("Menu", "Gameplay");
        let stack = d.build_arg_stack(&event);
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
    fn condition_display_any_scene_when_no_config() {
        let d = SCENE_CURRENT_CHANGED;
        assert_eq!(
            d.condition_display(&BTreeMap::new()),
            messages::CONDITION_ANY_SCENE.into()
        );
    }

    #[test]
    fn condition_display_shows_scene_name() {
        let d = SCENE_CURRENT_CHANGED;
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Gameplay".to_owned()))]);
        assert_eq!(
            d.condition_display(&config),
            messages::condition_scene("Gameplay").into()
        );
    }
}
