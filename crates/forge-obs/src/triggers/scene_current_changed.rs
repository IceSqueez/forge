use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{EventFilter, FormField, TriggerCategory, TriggerKindDescriptor};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SceneCurrentChangedDescriptor;

impl TriggerKindDescriptor for SceneCurrentChangedDescriptor {
    fn id(&self) -> &str {
        "obs.scenes.current_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene changed"
    }

    fn summary(&self) -> &str {
        "Fires when the current OBS program scene changes."
    }

    fn search_text(&self) -> &str {
        "obs scene changed current program switch"
    }

    fn icon_name(&self) -> &str {
        "layers-subtract"
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Optional {
            key: "scene",
            label: "Scene name (leave empty to match any)",
            inner: Box::new(FormField::DynamicSelect {
                key: "scene",
                label: "Scene",
                options_key: "obs.scene_names",
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        match config.get("scene") {
            Some(Variant::String(s)) if !s.is_empty() => format!("scene = {s}"),
            _ => "any scene".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("scene.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "scene.changed" {
            return false;
        }
        match config.get("scene") {
            Some(Variant::String(s)) if !s.is_empty() => {
                event.payload.get("to_scene").and_then(|v| v.as_str()) == Some(s.as_str())
            }
            _ => true,
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(to) = event.payload.get("to_scene").and_then(|v| v.as_str()) {
            stack = stack.set("obs.scene".to_owned(), Variant::String(to.to_owned()));
        }
        if let Some(from) = event.payload.get("from_scene").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.previous_scene".to_owned(),
                Variant::String(from.to_owned()),
            );
        }
        stack
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_types::{ActionId, Trigger, TriggerId};
    use serde_json::json;

    fn any_trigger() -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "obs.scenes.current_changed".to_owned(),
            config: BTreeMap::new(),
        }
    }

    fn named_trigger(scene: &str) -> Trigger {
        Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "obs.scenes.current_changed".to_owned(),
            config: BTreeMap::from([("scene".to_owned(), Variant::String(scene.to_owned()))]),
        }
    }

    fn scene_changed_event(from: &str, to: &str) -> Event {
        Event::new(
            EventSource::Obs,
            "scene.changed",
            json!({ "from_scene": from, "to_scene": to }),
        )
    }

    #[test]
    fn id_is_canonical() {
        assert_eq!(
            SceneCurrentChangedDescriptor.id(),
            "obs.scenes.current_changed"
        );
    }

    #[test]
    fn matches_any_scene_when_config_empty() {
        let d = SceneCurrentChangedDescriptor;
        let trigger = any_trigger();
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(d.matches_trigger(&trigger.config, &event));
    }

    #[test]
    fn matches_exact_scene_name() {
        let d = SceneCurrentChangedDescriptor;
        let trigger = named_trigger("Gameplay");
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(d.matches_trigger(&trigger.config, &event));
    }

    #[test]
    fn does_not_match_different_scene_name() {
        let d = SceneCurrentChangedDescriptor;
        let trigger = named_trigger("BRB");
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(!d.matches_trigger(&trigger.config, &event));
    }

    #[test]
    fn empty_string_scene_config_matches_any() {
        let d = SceneCurrentChangedDescriptor;
        let trigger = named_trigger("");
        let event = scene_changed_event("Menu", "Gameplay");
        assert!(d.matches_trigger(&trigger.config, &event));
    }

    #[test]
    fn does_not_match_non_scene_changed_event() {
        let d = SceneCurrentChangedDescriptor;
        let trigger = any_trigger();
        let event = Event::new(EventSource::Obs, "scene.created", json!({}));
        assert!(!d.matches_trigger(&trigger.config, &event));
    }

    #[test]
    fn build_arg_stack_populates_scene_keys() {
        let d = SceneCurrentChangedDescriptor;
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
        let d = SceneCurrentChangedDescriptor;
        assert_eq!(d.condition_display(&BTreeMap::new()), "any scene");
    }

    #[test]
    fn condition_display_shows_scene_name() {
        let d = SceneCurrentChangedDescriptor;
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Gameplay".to_owned()))]);
        assert_eq!(d.condition_display(&config), "scene = Gameplay");
    }
}
