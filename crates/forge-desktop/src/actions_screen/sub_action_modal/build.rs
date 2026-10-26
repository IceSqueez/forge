use super::field::{SelectDependency, SubFormField, is_var_key};
use super::unit_amount::build_unit_amount_field;
use super::*;
use crate::amount_scale::AmountScale;
use crate::config_field_label::{field_label, field_placeholder};
use crate::config_form::{
    code_field_editor, dependent_options, optional_gate_seed, optional_inner_seed,
    optional_shares_key,
};
use crate::motion_labels::preset_label;
use forge_components::int_entry_invalid;
use forge_components::{FONT_SM, InputEvent};
use forge_registry::{CodeKind, CodeLanguage, FormField};
use forge_types::{SubActionConfig, Variant, normalize_var_name};
use std::ops::RangeInclusive;

const VAR_PLACEHOLDER: &str = "%result%";
const INTEGER_PLACEHOLDER: &str = "0";

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

pub(super) struct FieldBuildContext<'a> {
    pub(super) kind_id: &'a str,
    pub(super) palette: ForgePalette,
    pub(super) options_map: &'a HashMap<String, Vec<(String, String)>>,
}

pub(super) fn build_form_fields(
    specs: &[FormField],
    config: &SubActionConfig,
    copy: &FieldBuildContext<'_>,
    cx: &mut Context<EditSubActionForm>,
) -> Vec<SubFormField> {
    let mut fields: Vec<SubFormField> = Vec::new();
    for spec in specs {
        push_form_field(spec, None, config, copy, &mut fields, cx);
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

#[expect(
    clippy::too_many_arguments,
    reason = "render helper takes each displayed value and handler separately"
)]
fn build_input_field(
    key: &str,
    label: &str,
    placeholder: String,
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
        .map(crate::config_text::editable_line)
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
        let ph = if is_var {
            VAR_PLACEHOLDER.to_owned()
        } else {
            placeholder
        };
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

#[expect(
    clippy::too_many_arguments,
    reason = "render helper takes each displayed value and handler separately"
)]
fn build_code_field(
    key: &str,
    label: &str,
    gate: Option<String>,
    language: CodeLanguage,
    kind: CodeKind,
    config: &SubActionConfig,
    palette: ForgePalette,
    cx: &mut Context<EditSubActionForm>,
) -> SubFormField {
    let editor = code_field_editor(language, kind, config_seed(config, key), palette, cx);
    SubFormField::Code {
        key: key.to_owned(),
        label: label.to_owned(),
        gate,
        language,
        kind,
        editor,
    }
}

fn config_seed(config: &SubActionConfig, key: &str) -> String {
    config
        .get(key)
        .map(crate::config_text::editable_text)
        .unwrap_or_default()
}

pub(super) fn push_form_field(
    spec: &FormField,
    gate: Option<String>,
    config: &SubActionConfig,
    copy: &FieldBuildContext<'_>,
    out: &mut Vec<SubFormField>,
    cx: &mut Context<EditSubActionForm>,
) {
    let palette = copy.palette;
    let label = field_label(copy.kind_id, spec);
    match spec {
        FormField::Text {
            key, placeholder, ..
        } => out.push(build_input_field(
            key,
            &label,
            field_placeholder(copy.kind_id, key, placeholder),
            None,
            false,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::TextArea { key, .. } => {
            out.push(build_area_field(key, &label, gate, config, palette, cx))
        }
        FormField::Code {
            key,
            language,
            kind,
            ..
        } => out.push(build_code_field(
            key, &label, gate, *language, *kind, config, palette, cx,
        )),
        FormField::Integer { key, min, max, .. } | FormField::Slider { key, min, max, .. } => out
            .push(build_input_field(
                key,
                &label,
                INTEGER_PLACEHOLDER.to_owned(),
                Some(*min..=*max),
                false,
                false,
                gate,
                config,
                palette,
                cx,
            )),
        FormField::UnitAmount { key, .. } | FormField::Duration { key, .. } => {
            if let Some(scale) = AmountScale::of(spec) {
                out.push(build_unit_amount_field(
                    key, &label, scale, gate, config, palette, cx,
                ));
            }
        }
        FormField::FilePicker { key, .. } => out.push(build_input_field(
            key,
            &label,
            String::new(),
            None,
            true,
            false,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::DateTime { key, .. } => out.push(build_input_field(
            key,
            &label,
            String::new(),
            None,
            false,
            true,
            gate,
            config,
            palette,
            cx,
        )),
        FormField::Select { key, options, .. } | FormField::Swatch { key, options, .. } => {
            let selected = config
                .get(*key)
                .map(crate::config_text::editable_line)
                .unwrap_or_default();
            let options = options
                .iter()
                .map(|opt| ((*opt).to_owned(), preset_label(key, opt)))
                .collect();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label,
                options_key: None,
                options,
                gate,
                selected,
                dependency: None,
            });
        }
        FormField::DynamicSelect {
            key, options_key, ..
        } => {
            let selected = config
                .get(*key)
                .map(crate::config_text::editable_line)
                .unwrap_or_default();
            let options = copy
                .options_map
                .get(*options_key)
                .cloned()
                .unwrap_or_default();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label,
                options_key: Some((*options_key).to_owned()),
                options,
                gate,
                selected,
                dependency: None,
            });
        }
        FormField::DependentSelect {
            key,
            options_prefix,
            depends_on,
            ..
        } => {
            let selected = config
                .get(*key)
                .map(crate::config_text::editable_line)
                .unwrap_or_default();
            let sibling = config
                .get(*depends_on)
                .map(crate::config_text::editable_line)
                .unwrap_or_default();
            out.push(SubFormField::Select {
                key: (*key).to_owned(),
                label,
                options_key: None,
                options: dependent_options(copy.options_map, options_prefix, &sibling),
                gate,
                selected,
                dependency: Some(SelectDependency {
                    options_prefix: (*options_prefix).to_owned(),
                    depends_on: (*depends_on).to_owned(),
                }),
            });
        }
        FormField::Toggle { key, .. } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label,
                gate,
                value,
                presence_gate: false,
            });
        }
        FormField::SubChain { .. } | FormField::CaseList { .. } => {
            out.push(SubFormField::Hint { label });
        }
        FormField::Optional { key, inner, .. } => {
            out.push(SubFormField::Bool {
                key: (*key).to_owned(),
                label,
                gate: gate.clone(),
                value: optional_gate_seed(key, inner, config),
                presence_gate: optional_shares_key(key, inner),
            });
            push_form_field(
                inner,
                Some((*key).to_owned()),
                &optional_inner_seed(key, inner, config),
                copy,
                out,
                cx,
            );
        }
    }
}
