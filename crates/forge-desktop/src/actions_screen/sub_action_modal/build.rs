use super::field::{SelectDependency, SubFormField, is_var_key};
use super::unit_amount::build_unit_amount_field;
use super::*;
use crate::amount_scale::AmountScale;
use crate::config_field_label::localized_label;
use crate::config_form::{code_field_editor, dependent_options};
use crate::quick_action_field_rows::int_entry_invalid;
use forge_components::{FONT_SM, InputEvent};
use forge_registry::{CodeLanguage, FormField};
use forge_types::{SubActionConfig, Variant, normalize_var_name};
use std::ops::RangeInclusive;

impl EditSubActionForm {
    fn on_var_input_event(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            let invalid = !text.trim().is_empty() && normalize_var_name(text).is_none();
            field.update(cx, |input, cx| input.set_invalid(invalid, cx));
        }
    }
}

pub(super) fn build_form_fields(
    specs: &[FormField],
    config: &SubActionConfig,
    palette: ForgePalette,
    options_map: &HashMap<String, Vec<(String, String)>>,
    cx: &mut Context<EditSubActionForm>,
) -> Vec<SubFormField> {
    let mut fields: Vec<SubFormField> = Vec::new();
    for spec in specs {
        push_form_field(spec, None, config, palette, options_map, &mut fields, cx);
    }
    fields
}

pub(super) fn build_step_meta_inputs(
    kind_label: &str,
    name_value: &str,
    condition_value: &str,
    cx: &mut Context<EditSubActionForm>,
) -> (Entity<TextInput>, Entity<TextInput>) {
    let palette = cx.palette();
    let placeholder = kind_label.to_owned();
    let name_value = name_value.to_owned();
    let condition_value = condition_value.to_owned();
    let name_input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx)
            .with_palette(palette)
            .plain()
            .with_font_size(FONT_SM);
        if !name_value.is_empty() {
            input.set_content(name_value, cx);
        }
        input
    });
    let condition_input = cx.new(|cx| {
        let mut input = TextInput::new("%user.isMod% == true", cx)
            .with_palette(palette)
            .mono()
            .prefix("if");
        if !condition_value.is_empty() {
            input.set_content(condition_value, cx);
        }
        input
    });
    (name_input, condition_input)
}

#[allow(clippy::too_many_arguments)]
fn build_input_field(
    key: &str,
    label: &str,
    placeholder: &'static str,
    integer: Option<RangeInclusive<i64>>,
    browse: bool,
    datetime: bool,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let seed = config
        .get(key)
        .map(forge_types::display_scalar)
        .unwrap_or_default();
    let is_var = is_var_key(key);
    let invalid_seed = if is_var {
        !seed.trim().is_empty() && normalize_var_name(&seed).is_none()
    } else {
        integer
            .as_ref()
            .is_some_and(|range| int_entry_invalid(&seed, *range.start(), *range.end(), false))
    };
    let input = cx.new(|cx| {
        let ph = if is_var { "%result%" } else { placeholder };
        let mut input = TextInput::new(ph, cx).with_palette(palette);
        if is_var {
            input = input
                .mono()
                .leading_icon(Icon::Variable, palette.warning)
                .accent(palette.warning);
        }
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input
    });
    if invalid_seed {
        input.update(cx, |input, cx| input.set_invalid(true, cx));
    }
    let sub = if is_var {
        Some(cx.subscribe(&input, EditSubActionForm::on_var_input_event))
    } else {
        integer.clone().map(|range| {
            cx.subscribe(
                &input,
                move |_, field: Entity<TextInput>, event: &InputEvent, cx| {
                    if let InputEvent::Changed(text) = event {
                        let invalid = int_entry_invalid(text, *range.start(), *range.end(), false);
                        field.update(cx, |input, cx| input.set_invalid(invalid, cx));
                    }
                },
            )
        })
    };
    SubFormField::Input {
        key: key.to_owned(),
        label: label.to_owned(),
        integer,
        browse,
        datetime,
        gate,
        input,
        _sub: sub,
    }
}

fn build_area_field(
    key: &str,
    label: &str,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let seed = config_seed(config, key);
    let area = cx.new(|cx| {
        let mut area = TextArea::new("", cx)
            .with_palette(palette)
            .with_height(SUB_AREA_FIELD_H);
        if !seed.is_empty() {
            area.set_content(seed, cx);
        }
        area
    });
    SubFormField::Area {
        key: key.to_owned(),
        label: label.to_owned(),
        gate,
        area,
    }
}

fn build_code_field(
    key: &str,
    label: &str,
    gate: Option<String>,
    language: CodeLanguage,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let editor = code_field_editor(language, config_seed(config, key), palette, cx);
    SubFormField::Code {
        key: key.to_owned(),
        label: label.to_owned(),
        gate,
        language,
        editor,
    }
}

fn config_seed(config: &SubActionConfig, key: &str) -> String {
    config
        .get(key)
        .map(forge_types::display_scalar)
        .unwrap_or_default()
}

pub(super) fn push_form_field(
    spec: &FormField,
    gate: Option<String>,
    config: &SubActionConfig,
    palette: ForgePalette,
    options_map: &HashMap<String, Vec<(String, String)>>,
    out: &mut Vec<SubFormField>,
    cx: &mut Context<EditSubActionForm>,
) {
    match spec {
        FormField::Text {
            key,
            label,
            placeholder,
        } => out.push(build_input_field(
            key,
            label,
            placeholder,
            None,
            false,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::TextArea { key, label } => {
            out.push(build_area_field(key, label, gate, config, palette, cx))
        }
        FormField::Code {
            key,
            label,
            language,
        } => out.push(build_code_field(
            key, label, gate, *language, config, palette, cx,
        )),
        FormField::Integer {
            key,
            label,
            min,
            max,
        }
        | FormField::Slider {
            key,
            label,
            min,
            max,
            ..
        } => out.push(build_input_field(
            key,
            label,
            "0",
            Some(*min..=*max),
            false,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::UnitAmount { key, label, .. } | FormField::Duration { key, label, .. } => {
            if let Some(scale) = AmountScale::of(spec) {
                out.push(build_unit_amount_field(
                    key, label, scale, gate, config, palette, cx,
                ));
            }
        }
        FormField::FilePicker { key, label } => out.push(build_input_field(
            key, label, "", None, true, false, gate, config, palette, cx,
        )),
        FormField::DateTime { key, label } => out.push(build_input_field(
            key, label, "", None, false, true, gate, config, palette, cx,
        )),
        FormField::Select {
            key,
            label,
            options,
        }
        | FormField::Swatch {
            key,
            label,
            options,
        } => {
            let selected = config
                .get(*key)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            let options = options
                .iter()
                .map(|opt| ((*opt).to_owned(), (*opt).to_owned()))
                .collect();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                options_key: None,
                options,
                gate,
                selected,
                dependency: None,
            });
        }
        FormField::DynamicSelect {
            key,
            label,
            options_key,
        } => {
            let selected = config
                .get(*key)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            let options = options_map.get(*options_key).cloned().unwrap_or_default();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label: localized_label(label),
                options_key: Some((*options_key).to_owned()),
                options,
                gate,
                selected,
                dependency: None,
            });
        }
        FormField::DependentSelect {
            key,
            label,
            options_prefix,
            depends_on,
        } => {
            let selected = config
                .get(*key)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            let sibling = config
                .get(*depends_on)
                .map(forge_types::display_scalar)
                .unwrap_or_default();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                options_key: None,
                options: dependent_options(options_map, options_prefix, &sibling),
                gate,
                selected,
                dependency: Some(SelectDependency {
                    options_prefix: (*options_prefix).to_owned(),
                    depends_on: (*depends_on).to_owned(),
                }),
            });
        }
        FormField::Toggle { key, label } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                gate,
                value,
            });
        }
        FormField::SubChain { label, .. } | FormField::CaseList { label, .. } => {
            out.push(SubFormField::Hint {
                label: (*label).to_owned(),
            });
        }
        FormField::Optional { key, label, inner } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label: (*label).to_owned(),
                gate: gate.clone(),
                value,
            });
            push_form_field(
                inner,
                Some((*key).to_owned()),
                config,
                palette,
                options_map,
                out,
                cx,
            );
        }
    }
}
