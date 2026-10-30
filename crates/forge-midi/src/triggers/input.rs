use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
    joined_condition,
};
use forge_types::{
    ArgStack, DeclaredVariable, MessageCode, MessageRef, SynthesisHint, TriggerConfig, UserText,
    VariableSchema, Variant, VariantKind,
};
use serde_json::Value;

use super::config_read::{config_filter_text, config_int};
use crate::vocabulary::fields::input as fields;
use crate::vocabulary::limits::{CHANNEL_MAX, DATA_BYTE_MAX};
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, config_keys, event_kinds, messages};

pub(crate) struct SignalNumber {
    pub(crate) config_key: &'static str,
    pub(crate) optional_label: &'static str,
    pub(crate) label: &'static str,
    pub(crate) filter_condition: MessageCode,
    pub(crate) any_condition: MessageCode,
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
            inner: Box::new(FormField::OneBasedInteger {
                key: config_keys::CHANNEL,
                label: "Channel",
                count: CHANNEL_MAX + 1,
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

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        let number = self.number.as_ref();
        let number_filter = number.and_then(|number| {
            config_int(config, number.config_key).map(|value| {
                MessageRef::new(number.filter_condition.clone())
                    .with_arg(messages::NUMBER_CONDITION_ARG, value)
            })
        });
        let channel_filter =
            config_int(config, config_keys::CHANNEL).map(messages::condition_channel);
        let mut parts: Vec<MessageRef> = number_filter.into_iter().chain(channel_filter).collect();
        if parts.is_empty() {
            parts.extend(number.map(|number| MessageRef::new(number.any_condition.clone())));
            parts.push(messages::CONDITION_ANY_CHANNEL.into());
        }
        parts.extend(
            config_filter_text(config, config_keys::DEVICE).map(messages::condition_device),
        );
        joined_condition(parts)
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
