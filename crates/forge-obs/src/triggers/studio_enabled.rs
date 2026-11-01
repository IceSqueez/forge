use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::{event_kinds, messages, trigger_ids};

pub(crate) const STUDIO_ENABLED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STUDIO_ENABLED,
    source: EventSource::Obs,
    label: "OBS Studio Mode enabled",
    summary: "Fires when OBS Studio Mode is turned on.",
    search_text: "obs studio mode enabled on preview program",
    icon: "layout-columns",
    prefix: event_kinds::STUDIO_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::STUDIO_ENABLED),
    condition: TriggerCondition::Fixed(messages::CONDITION_STUDIO_MODE_ENABLED),
    config_fields: no_config_fields,
    variables: families::studio_variables,
};

#[cfg(test)]
mod tests {
    use super::super::lifecycle;
    use super::super::studio_enabled::STUDIO_ENABLED;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn studio_event(kind: &str) -> Event {
        Event::new(EventSource::Obs, kind, json!({}))
    }

    #[test]
    fn each_studio_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 2] = [
            ("obs.studio.enabled", &STUDIO_ENABLED),
            ("obs.studio.disabled", &lifecycle::STUDIO_DISABLED),
        ];
        let kinds = ["obs.studio.enabled", "obs.studio.disabled"];
        for (own_kind, descriptor) in descriptors {
            for kind in kinds {
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &studio_event(kind)),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn studio_descriptor_rejects_non_studio_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        let cfg = BTreeMap::new();
        assert!(!STUDIO_ENABLED.matches_trigger(&cfg, &event));
        assert!(!lifecycle::STUDIO_DISABLED.matches_trigger(&cfg, &event));
    }

    #[test]
    fn arg_stack_derives_enabled_flag_from_kind_with_empty_payload() {
        for (kind, expected) in [("obs.studio.enabled", true), ("obs.studio.disabled", false)] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            let stack = STUDIO_ENABLED.build_arg_stack(&event);
            assert_eq!(
                stack.get("obs.studio.enabled"),
                Some(&Variant::Bool(expected)),
                "kind {kind}",
            );
        }
    }
}
