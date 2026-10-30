use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};
use serde_json::Value;

use super::config_read::config_filter_text;
use crate::vocabulary::fields::port as fields;
use crate::vocabulary::variables::device as variables;
use crate::vocabulary::{config_keys, event_kinds, messages};

pub(crate) struct MidiPortTrigger {
    pub(crate) id: &'static str,
    pub(crate) event_kind: &'static str,
    pub(crate) label: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) search_text: &'static str,
}

impl MidiPortTrigger {
    fn text<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
        event.payload.get(key).and_then(Value::as_str)
    }
}

impl TriggerKindDescriptor for MidiPortTrigger {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        self.label
    }

    fn summary(&self) -> &str {
        self.summary
    }

    fn search_text(&self) -> &str {
        self.search_text
    }

    fn icon_name(&self) -> &str {
        "music"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Optional {
            key: config_keys::DIRECTION,
            label: "Direction (leave empty for any)",
            inner: Box::new(FormField::Select {
                key: config_keys::DIRECTION,
                label: "Direction",
                options: &fields::direction::ALL,
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        match config_filter_text(config, config_keys::DIRECTION) {
            Some(direction) => messages::condition_direction(direction).into(),
            None => messages::CONDITION_ANY_DIRECTION.into(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some(event_kinds::PORT_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != self.event_kind {
            return false;
        }
        match config_filter_text(config, config_keys::DIRECTION) {
            Some(direction) => Self::text(event, fields::DIRECTION) == Some(direction),
            None => true,
        }
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = Self::text(event, fields::PORT_NAME) {
            stack = stack.set(variables::NAME.to_owned(), Variant::String(name.to_owned()));
        }
        if let Some(direction) = Self::text(event, fields::DIRECTION) {
            stack = stack.set(
                variables::DIRECTION.to_owned(),
                Variant::String(direction.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: variables::NAME.to_owned(),
                    kind: VariantKind::String,
                    label: "Device name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::DIRECTION.to_owned(),
                    kind: VariantKind::String,
                    label: "Direction".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}
