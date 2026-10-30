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
            .unwrap_or_default();
        configured_name == event_name
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
