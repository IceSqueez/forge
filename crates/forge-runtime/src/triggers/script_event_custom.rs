use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, UserText, Variant};

use crate::core_messages;

pub struct ScriptEventCustomDescriptor;

impl TriggerKindDescriptor for ScriptEventCustomDescriptor {
    fn id(&self) -> &str {
        "script.event.custom"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Core
    }

    fn label(&self) -> &str {
        "Custom Script Event"
    }

    fn summary(&self) -> &str {
        "Fires when a rhai script publishes a named custom event"
    }

    fn search_text(&self) -> &str {
        "script event custom rhai code trigger"
    }

    fn icon_name(&self) -> &str {
        "bolt"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("event_name".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "event_name",
            label: "Event Name",
            placeholder: "my_event",
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        config
            .get("event_name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(
                || core_messages::CONDITION_ANY_EVENT.into(),
                |event| core_messages::condition_event(event).into(),
            )
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Server),
            kind_prefix: Some("custom.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let event_name = match event.kind.strip_prefix("custom.") {
            Some(n) => n,
            None => return false,
        };
        let configured_name = config
            .get("event_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim();
        configured_name.is_empty() || configured_name == event_name
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let Some(obj) = event.payload.as_object() else {
            return ArgStack::new();
        };
        obj.iter()
            .filter_map(|(k, v)| Variant::from_json(v.clone()).ok().map(|vv| (k.clone(), vv)))
            .fold(ArgStack::new(), |stack, (k, v)| stack.set(k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_event(kind: &str) -> Event {
        Event::new(EventSource::Server, kind, serde_json::Value::Null)
    }

    fn named(event_name: &str) -> TriggerConfig {
        TriggerConfig::from([(
            "event_name".to_owned(),
            Variant::String(event_name.to_owned()),
        )])
    }

    #[test]
    fn a_blank_event_name_fires_on_every_custom_event() {
        for configured in ["", "   ", "\t\n"] {
            assert!(
                ScriptEventCustomDescriptor
                    .matches_trigger(&named(configured), &custom_event("custom.raid")),
                "{configured:?}"
            );
        }
    }

    #[test]
    fn an_absent_event_name_fires_on_every_custom_event() {
        assert!(
            ScriptEventCustomDescriptor
                .matches_trigger(&TriggerConfig::new(), &custom_event("custom.raid"))
        );
    }

    #[test]
    fn a_padded_event_name_fires_only_on_that_name() {
        for (kind, fires) in [
            ("custom.raid", true),
            ("custom.raids", false),
            ("custom.", false),
        ] {
            assert_eq!(
                ScriptEventCustomDescriptor.matches_trigger(&named("  raid "), &custom_event(kind)),
                fires,
                "{kind}"
            );
        }
    }

    #[test]
    fn a_blank_event_name_still_ignores_events_outside_the_custom_namespace() {
        assert!(!ScriptEventCustomDescriptor.matches_trigger(&named(""), &custom_event("raid")));
    }

    #[test]
    fn the_condition_shows_the_trimmed_name_or_any_event() {
        for (configured, expected) in [
            (
                "  raid ",
                UserText::from(core_messages::condition_event("raid")),
            ),
            ("   ", core_messages::CONDITION_ANY_EVENT.into()),
        ] {
            assert_eq!(
                ScriptEventCustomDescriptor.condition_display(&named(configured)),
                expected,
                "{configured:?}"
            );
        }
    }
}
