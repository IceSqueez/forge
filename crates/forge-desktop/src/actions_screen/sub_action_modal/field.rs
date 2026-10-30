use super::*;
use crate::amount_scale::AmountScale;
use forge_components::CodeEditor;
use forge_registry::{CodeKind, CodeLanguage};
use forge_types::{Variant, normalize_var_name};
use std::ops::RangeInclusive;

impl InputFlavor {
    pub(super) fn shown_bounds(&self) -> Option<RangeInclusive<i64>> {
        match self {
            Self::Integer(range) => Some(range.clone()),
            Self::OneBased(count) => Some(1..=*count),
            Self::Text | Self::File | Self::DateTime | Self::VariableName => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum InputFlavor {
    Text,
    Integer(RangeInclusive<i64>),
    OneBased(i64),
    File,
    DateTime,
    VariableName,
}

pub(super) enum SubFormField {
    Input {
        key: String,
        label: String,
        flavor: InputFlavor,
        gate: Option<String>,
        input: Entity<TextInput>,
        _sub: Option<Subscription>,
    },
    Area {
        key: String,
        label: String,
        gate: Option<String>,
        area: Entity<TextArea>,
    },
    Code {
        key: String,
        label: String,
        gate: Option<String>,
        language: CodeLanguage,
        kind: CodeKind,
        editor: Entity<CodeEditor>,
    },
    Bool {
        key: String,
        label: String,
        gate: Option<String>,
        value: bool,
        presence_gate: bool,
    },
    Select {
        key: String,
        label: String,
        options_key: Option<String>,
        options: Vec<(String, String)>,
        gate: Option<String>,
        selected: String,
        dependency: Option<SelectDependency>,
    },
    UnitAmount {
        key: String,
        label: String,
        unit_picker_key: String,
        scale: AmountScale,
        unit: String,
        gate: Option<String>,
        input: Entity<TextInput>,
        _sub: Subscription,
    },
    Hint {
        label: String,
    },
}

pub(super) struct SelectEntries {
    pub(super) options: Vec<(String, String)>,
    pub(super) selected: String,
    pub(super) accepts_typed_value: bool,
}

impl SubFormField {
    pub(super) fn accepts_variables(&self) -> bool {
        match self {
            Self::Input { flavor, .. } => *flavor == InputFlavor::Text,
            Self::Area { .. }
            | Self::Code {
                kind: CodeKind::Expression,
                ..
            } => true,
            _ => false,
        }
    }

    pub(super) fn select_entries(&self, key: &str) -> Option<SelectEntries> {
        match self {
            Self::Select {
                key: k,
                options,
                selected,
                options_key,
                ..
            } if k == key => Some(SelectEntries {
                options: options.clone(),
                selected: selected.clone(),
                accepts_typed_value: options_key.is_some(),
            }),
            Self::UnitAmount {
                unit_picker_key,
                scale,
                unit,
                ..
            } if unit_picker_key == key => Some(SelectEntries {
                options: scale.unit_options(),
                selected: unit.clone(),
                accepts_typed_value: false,
            }),
            _ => None,
        }
    }
}

pub(super) struct SelectDependency {
    pub(super) options_prefix: String,
    pub(super) depends_on: String,
}

pub(super) fn field_gate(field: &SubFormField) -> Option<&String> {
    match field {
        SubFormField::Input { gate, .. }
        | SubFormField::Area { gate, .. }
        | SubFormField::Code { gate, .. }
        | SubFormField::Bool { gate, .. }
        | SubFormField::Select { gate, .. }
        | SubFormField::UnitAmount { gate, .. } => gate.as_ref(),
        SubFormField::Hint { .. } => None,
    }
}

pub(super) fn field_keys(field: &SubFormField) -> Vec<&str> {
    match field {
        SubFormField::Input { key, .. }
        | SubFormField::Area { key, .. }
        | SubFormField::Code { key, .. }
        | SubFormField::Bool { key, .. }
        | SubFormField::Select { key, .. } => vec![key.as_str()],
        SubFormField::UnitAmount { key, scale, .. } => scale.stored_keys(key),
        SubFormField::Hint { .. } => Vec::new(),
    }
}

pub(super) fn field_values(field: &SubFormField, cx: &App) -> Vec<(String, Variant)> {
    let SubFormField::UnitAmount {
        key,
        scale,
        unit,
        input,
        ..
    } = field
    else {
        return field_value(field, cx).into_iter().collect();
    };
    let amount = input.read(cx).content().trim().parse::<i64>().ok();
    scale.stored_values(key, unit, amount)
}

fn field_value(field: &SubFormField, cx: &App) -> Option<(String, Variant)> {
    match field {
        SubFormField::Input {
            key, flavor, input, ..
        } => {
            let text = input.read(cx).content().to_owned();
            match flavor {
                InputFlavor::Integer(_) => {
                    let parsed = text.trim().parse::<i64>().ok()?;
                    Some((key.clone(), Variant::Int(parsed)))
                }
                InputFlavor::OneBased(_) => {
                    let parsed = text.trim().parse::<i64>().ok()?;
                    Some((key.clone(), Variant::Int(parsed - 1)))
                }
                InputFlavor::VariableName => {
                    let name = normalize_var_name(&text).unwrap_or_default();
                    Some((key.clone(), Variant::String(name)))
                }
                InputFlavor::Text | InputFlavor::File | InputFlavor::DateTime => {
                    Some((key.clone(), Variant::String(text)))
                }
            }
        }
        SubFormField::Area { key, area, .. } => Some((
            key.clone(),
            Variant::String(area.read(cx).content().to_owned()),
        )),
        SubFormField::Code { key, editor, .. } => Some((
            key.clone(),
            Variant::String(editor.read(cx).content().to_owned()),
        )),
        SubFormField::Bool {
            presence_gate: true,
            ..
        } => None,
        SubFormField::Bool { key, value, .. } => Some((key.clone(), Variant::Bool(*value))),
        SubFormField::Select { key, selected, .. } => {
            Some((key.clone(), Variant::String(selected.clone())))
        }
        SubFormField::UnitAmount { .. } | SubFormField::Hint { .. } => None,
    }
}
