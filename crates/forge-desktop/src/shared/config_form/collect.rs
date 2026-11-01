use std::collections::HashMap;

use forge_types::{Variant, normalize_var_name};
use gpui::App;

use super::fold::within;
use super::{ConfigField, FieldConfig, InputFlavor};

pub(crate) struct GateStates<'a>(HashMap<&'a str, bool>);

impl<'a> GateStates<'a> {
    pub(crate) fn of(fields: &'a [ConfigField]) -> Self {
        Self(
            fields
                .iter()
                .filter_map(|field| match field {
                    ConfigField::Bool { key, value, .. } => Some((key.as_str(), *value)),
                    _ => None,
                })
                .collect(),
        )
    }

    pub(crate) fn open(&self, gate: Option<&String>) -> bool {
        gate.is_none_or(|gate| self.0.get(gate.as_str()).copied().unwrap_or(false))
    }
}

pub(crate) struct CollectedValues {
    pub(crate) values: Vec<(String, Variant)>,
    pub(crate) cleared: Vec<String>,
}

fn integer_value(key: &str, text: &str, flavor: &InputFlavor) -> Option<(String, Variant)> {
    let number = text.trim().parse::<i64>().ok()?;
    let stored = match flavor.shown_bounds() {
        Some(range) => within(number, &range),
        None => number,
    };
    let offset = i64::from(matches!(flavor, InputFlavor::OneBased(_)));
    Some((key.to_owned(), Variant::Int(stored - offset)))
}

pub(crate) fn field_entries(field: &ConfigField, cx: &App) -> Vec<(String, Variant)> {
    let single = |key: &String, value: Variant| vec![(key.clone(), value)];
    match field {
        ConfigField::Bool {
            presence_gate: true,
            ..
        }
        | ConfigField::Hint { .. } => Vec::new(),
        ConfigField::Bool { key, value, .. } => single(key, Variant::Bool(*value)),
        ConfigField::Slide { key, value, .. } => single(key, Variant::Int(*value)),
        ConfigField::Swatch { selected, .. } if selected.is_empty() => Vec::new(),
        ConfigField::Swatch { key, selected, .. } | ConfigField::Choice { key, selected, .. } => {
            single(key, Variant::String(selected.clone()))
        }
        ConfigField::Code { key, editor, .. } => {
            single(key, Variant::String(editor.read(cx).content().to_owned()))
        }
        ConfigField::Area { key, area, .. } => {
            single(key, Variant::String(area.read(cx).content().to_owned()))
        }
        ConfigField::Input {
            key, flavor, input, ..
        } => {
            let text = input.read(cx).content();
            match flavor {
                InputFlavor::Integer(_) | InputFlavor::OneBased(_) => {
                    integer_value(key, text, flavor).into_iter().collect()
                }
                InputFlavor::VariableName => single(
                    key,
                    Variant::String(normalize_var_name(text).unwrap_or_default()),
                ),
                InputFlavor::Text | InputFlavor::File | InputFlavor::DateTime => {
                    single(key, Variant::String(text.to_owned()))
                }
            }
        }
        ConfigField::UnitAmount {
            key,
            scale,
            unit,
            input,
            ..
        } => {
            let amount = input
                .read(cx)
                .content()
                .trim()
                .parse::<i64>()
                .ok()
                .map(|number| within(number, &scale.range_for(unit)));
            scale.stored_values(key, unit, amount)
        }
    }
}

fn blank_optional_key<'a>(field: &'a ConfigField, cx: &App) -> Option<&'a str> {
    match field {
        ConfigField::Input {
            key,
            optional: true,
            input,
            ..
        } if input.read(cx).content().trim().is_empty() => Some(key.as_str()),
        _ => None,
    }
}

pub(crate) fn gated_field_values(fields: &[ConfigField], cx: &App) -> CollectedValues {
    let gates = GateStates::of(fields);
    let open_fields = || fields.iter().filter(|field| gates.open(field.gate()));
    let values = open_fields()
        .flat_map(|field| field_entries(field, cx))
        .collect();
    let dropped_gates: Vec<&String> = fields
        .iter()
        .filter_map(|field| match field {
            ConfigField::Bool {
                key,
                gate,
                value: false,
                presence_gate: true,
            } if gates.open(gate.as_ref()) => Some(key),
            _ => None,
        })
        .collect();
    let mut cleared: Vec<String> = fields
        .iter()
        .filter(|field| {
            field
                .gate()
                .is_some_and(|gate| dropped_gates.contains(&gate))
        })
        .flat_map(ConfigField::stored_keys)
        .chain(dropped_gates.iter().map(|gate| gate.as_str()))
        .chain(open_fields().filter_map(|field| blank_optional_key(field, cx)))
        .map(str::to_owned)
        .collect();
    cleared.sort();
    cleared.dedup();
    CollectedValues { values, cleared }
}

pub(crate) fn collect_field_values(fields: &[ConfigField], buffer: &mut FieldConfig, cx: &App) {
    let CollectedValues { values, cleared } = gated_field_values(fields, cx);
    buffer.extend(values);
    for key in cleared {
        buffer.remove(&key);
    }
}
