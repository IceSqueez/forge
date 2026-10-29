use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, SynthesisHint, TriggerConfig, VariableSchema, Variant, VariantKind,
};
use serde_json::Value;

use super::config_read::{config_filter_text, config_int};
use crate::vocabulary::fields::input as fields;
use crate::vocabulary::limits::{CHANNEL_MAX, DATA_BYTE_MAX};
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, config_keys, event_kinds};

pub(crate) struct SignalNumber {
    pub(crate) config_key: &'static str,
    pub(crate) optional_label: &'static str,
    pub(crate) label: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) any_display: &'static str,
    pub(crate) variable: &'static str,
    pub(crate) variable_label: &'static str,
}

pub(crate) struct SignalValue {
    pub(crate) variable: &'static str,
    pub(crate) label: &'static str,
    pub(crate) max: i64,
}

pub(crate) struct MidiInputTrigger {
    pub(crate) id: &'static str,
    pub(crate) kind: MidiSignalKind,
    pub(crate) label: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) search_text: &'static str,
    pub(crate) icon: &'static str,
    pub(crate) number: Option<SignalNumber>,
    pub(crate) value: Option<SignalValue>,
}

struct SignalReading<'a> {
    number: Option<i64>,
    value: Option<i64>,
    channel: Option<i64>,
    port_name: Option<&'a str>,
}

impl MidiInputTrigger {
    fn read<'a>(&self, event: &'a Event) -> SignalReading<'a> {
        let int_at = |key: Option<&str>| {
            key.and_then(|key| event.payload.get(key))
                .and_then(Value::as_i64)
        };
        SignalReading {
            number: int_at(self.kind.number_field()),
            value: int_at(self.kind.value_field()),
            channel: int_at(Some(fields::CHANNEL)),
            port_name: event.payload.get(fields::PORT_NAME).and_then(Value::as_str),
        }
    }
}

impl TriggerKindDescriptor for MidiInputTrigger {
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
        self.icon
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        let mut form = Vec::new();
        if let Some(number) = &self.number {
            form.push(FormField::Optional {
                key: number.config_key,
                label: number.optional_label,
                inner: Box::new(FormField::Integer {
                    key: number.config_key,
                    label: number.label,
                    min: 0,
                    max: DATA_BYTE_MAX,
                }),
            });
        }
        form.push(FormField::Optional {
            key: config_keys::CHANNEL,
            label: "Channel (leave empty for any)",
            inner: Box::new(FormField::Integer {
                key: config_keys::CHANNEL,
                label: "Channel",
                min: 0,
                max: CHANNEL_MAX,
            }),
        });
        form.push(FormField::Optional {
            key: config_keys::DEVICE,
            label: "Device (leave empty for any)",
            inner: Box::new(FormField::Text {
                key: config_keys::DEVICE,
                label: "Device",
                placeholder: "e.g. Launchkey Mini",
            }),
        });
        form
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let mut parts = Vec::new();
        if let Some(number) = &self.number
            && let Some(n) = config_int(config, number.config_key)
        {
            parts.push(format!("{}={n}", number.display_name));
        }
        if let Some(channel) = config_int(config, config_keys::CHANNEL) {
            parts.push(format!("ch={channel}"));
        }
        let mut display = if parts.is_empty() {
            match &self.number {
                Some(number) => format!("{}, any channel", number.any_display),
                None => "any channel".to_owned(),
            }
        } else {
            parts.join(", ")
        };
        if let Some(device) = config_filter_text(config, config_keys::DEVICE) {
            display.push_str(&format!(", device={device}"));
        }
        display
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some(event_kinds::INPUT_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != self.kind.event_kind() {
            return false;
        }
        let reading = self.read(event);
        if let Some(number) = &self.number
            && let Some(wanted) = config_int(config, number.config_key)
            && reading.number != Some(wanted)
        {
            return false;
        }
        if let Some(wanted) = config_int(config, config_keys::CHANNEL)
            && reading.channel != Some(wanted)
        {
            return false;
        }
        if let Some(device) = config_filter_text(config, config_keys::DEVICE)
            && reading.port_name != Some(device)
        {
            return false;
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let reading = self.read(event);
        let mut stack = ArgStack::new();
        if let (Some(number), Some(n)) = (&self.number, reading.number) {
            stack = stack.set(number.variable.to_owned(), Variant::Int(n));
        }
        if let (Some(value), Some(v)) = (&self.value, reading.value) {
            stack = stack.set(value.variable.to_owned(), Variant::Int(v));
        }
        if let Some(channel) = reading.channel {
            stack = stack.set(variables::CHANNEL.to_owned(), Variant::Int(channel));
        }
        if let Some(port) = reading.port_name {
            stack = stack.set(variables::PORT.to_owned(), Variant::String(port.to_owned()));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        let mut declared = Vec::new();
        if let Some(number) = &self.number {
            declared.push(DeclaredVariable {
                name: number.variable.to_owned(),
                kind: VariantKind::Int,
                label: number.variable_label.to_owned(),
                synthesis: Some(SynthesisHint::BoundedInt {
                    min: 0,
                    max: DATA_BYTE_MAX,
                }),
            });
        }
        if let Some(value) = &self.value {
            declared.push(DeclaredVariable {
                name: value.variable.to_owned(),
                kind: VariantKind::Int,
                label: value.label.to_owned(),
                synthesis: Some(SynthesisHint::BoundedInt {
                    min: 0,
                    max: value.max,
                }),
            });
        }
        declared.push(DeclaredVariable {
            name: variables::CHANNEL.to_owned(),
            kind: VariantKind::Int,
            label: "Channel".to_owned(),
            synthesis: Some(SynthesisHint::BoundedInt {
                min: 0,
                max: CHANNEL_MAX,
            }),
        });
        declared.push(DeclaredVariable {
            name: variables::PORT.to_owned(),
            kind: VariantKind::String,
            label: "Port name".to_owned(),
            synthesis: None,
        });
        Some(VariableSchema {
            variables: declared,
        })
    }
}
