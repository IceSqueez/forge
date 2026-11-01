use std::borrow::Cow;
use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use forge_components::{
    CodeEditor, Icon, InputEvent, Language, TextArea, TextInput, accent_swatch, int_entry_invalid,
    tr,
};
use forge_registry::{CodeKind, CodeLanguage, FormField};
use forge_types::{Variant, normalize_var_name};
use gpui::{App, Context, Entity, SharedString, Subscription, prelude::*};

use super::choices::{dependent_options, has_options_provider};
use super::{
    AREA_FIELD_H, CODE_FIELD_ROWS, ChoiceDependency, ChoiceSupport, ConfigField, EntryPolicy,
    FieldCapabilities, FieldConfig, FoldContext, INTEGER_PLACEHOLDER, InputFlavor, VAR_PLACEHOLDER,
};
use crate::automation::amount_scale::{AmountScale, AmountSeed};
use crate::shared::config_field_label::field_placeholder;
use crate::shared::option_choices::ChoiceOption;
use crate::shared::option_labels::option_label;

struct InputSpec<'a> {
    key: &'a str,
    placeholder: SharedString,
    flavor: InputFlavor,
    optional: bool,
}

fn plain_input(key: &str, placeholder: SharedString, flavor: InputFlavor) -> InputSpec<'_> {
    InputSpec {
        key,
        placeholder,
        flavor,
        optional: false,
    }
}

fn integer_input<V: 'static>(
    key: &str,
    flavor: InputFlavor,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let optional = ctx.capabilities.entry == EntryPolicy::Inline && !ctx.defaults.contains_key(key);
    let placeholder = if optional {
        SharedString::from(tr!("config_form_auto_placeholder"))
    } else {
        SharedString::from(INTEGER_PLACEHOLDER)
    };
    build_config_input(
        InputSpec {
            key,
            placeholder,
            flavor,
            optional,
        },
        gate,
        ctx,
        cx,
    )
}

fn choice_or_input<V: 'static>(
    key: &str,
    options: Option<Vec<ChoiceOption>>,
    options_key: Option<&str>,
    dependency: Option<ChoiceDependency>,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    match options {
        Some(options) => ConfigField::Choice {
            key: key.to_owned(),
            gate,
            options,
            options_key: options_key.map(str::to_owned),
            selected: read_text(ctx.config, key),
            dependency,
        },
        None => build_config_input(
            plain_input(key, SharedString::default(), InputFlavor::Text),
            gate,
            ctx,
            cx,
        ),
    }
}

fn labelled_options(kind_id: &str, key: &str, options: &[&str]) -> Vec<ChoiceOption> {
    options
        .iter()
        .map(|option| ChoiceOption::new(*option, option_label(kind_id, key, option)))
        .collect()
}

fn fold_select<V: 'static>(
    spec: &FormField,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> Option<ConfigField> {
    let choices = ctx.capabilities.choices;
    let field = match spec {
        FormField::Select { key, options, .. } => {
            let offered = matches!(choices, ChoiceSupport::Picker(_))
                .then(|| labelled_options(ctx.kind_id, key, options));
            choice_or_input(key, offered, None, None, gate, ctx, cx)
        }
        FormField::DynamicSelect {
            key, options_key, ..
        } => {
            let offered = match choices {
                ChoiceSupport::Picker(map) => {
                    Some(map.get(*options_key).cloned().unwrap_or_default())
                }
                ChoiceSupport::Text => has_options_provider(options_key).then(Vec::new),
            };
            choice_or_input(key, offered, Some(options_key), None, gate, ctx, cx)
        }
        FormField::DependentSelect {
            key,
            options_prefix,
            depends_on,
            ..
        } => {
            let offered = match choices {
                ChoiceSupport::Text => None,
                ChoiceSupport::Picker(map) => Some(dependent_options(
                    map,
                    options_prefix,
                    &read_text(ctx.config, depends_on),
                )),
            };
            let dependency = ChoiceDependency {
                options_prefix: (*options_prefix).to_owned(),
                depends_on: (*depends_on).to_owned(),
            };
            choice_or_input(key, offered, None, Some(dependency), gate, ctx, cx)
        }
        _ => return None,
    };
    Some(field)
}

fn fold_swatch<V: 'static>(
    key: &str,
    options: &[&str],
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    if !ctx.capabilities.swatches {
        let offered = Some(labelled_options(ctx.kind_id, key, options));
        return choice_or_input(key, offered, None, None, gate, ctx, cx);
    }
    ConfigField::Swatch {
        key: key.to_owned(),
        gate,
        options: options
            .iter()
            .map(|name| {
                let tint = accent_swatch(name, ctx.palette).unwrap_or(ctx.palette.text_faint);
                ((*name).to_owned(), tint)
            })
            .collect(),
        selected: read_text(ctx.config, key),
    }
}

fn fold_slider<V: 'static>(
    key: &str,
    range: RangeInclusive<i64>,
    unit: &'static str,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    if !ctx.capabilities.sliders {
        return integer_input(key, InputFlavor::Integer(range), gate, ctx, cx);
    }
    let (min, max) = (*range.start(), *range.end());
    ConfigField::Slide {
        key: key.to_owned(),
        gate,
        min,
        max,
        unit,
        value: within(read_int(ctx.config, key).unwrap_or(min), &range),
    }
}

fn fold_text_area<V: 'static>(
    key: &str,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    if !ctx.capabilities.text_areas {
        let spec = plain_input(key, SharedString::default(), InputFlavor::Text);
        return build_config_input(spec, gate, ctx, cx);
    }
    let seed = ctx
        .config
        .get(key)
        .map(crate::shared::config_text::editable_text)
        .unwrap_or_default();
    let area = cx.new(|cx| {
        let mut area = TextArea::new("", cx).with_height(AREA_FIELD_H);
        if !seed.is_empty() {
            area.set_content(seed, cx);
        }
        area
    });
    ConfigField::Area {
        key: key.to_owned(),
        gate,
        area,
    }
}

fn text_flavor(key: &str, capabilities: &FieldCapabilities<'_>) -> InputFlavor {
    if capabilities
        .output_name_keys
        .iter()
        .any(|output| output == key)
    {
        InputFlavor::VariableName
    } else {
        InputFlavor::Text
    }
}

fn fold_optional<V: 'static>(
    key: &str,
    inner: &FormField,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<V>,
) {
    out.push(ConfigField::Bool {
        key: key.to_owned(),
        gate,
        value: optional_gate_seed(key, inner, ctx.config),
        presence_gate: optional_shares_key(key, inner),
    });
    let inner_config = optional_inner_seed(key, inner, ctx.config);
    let inner_ctx = FoldContext {
        kind_id: ctx.kind_id,
        config: &inner_config,
        defaults: ctx.defaults,
        palette: ctx.palette,
        capabilities: ctx.capabilities,
        on_committed: ctx.on_committed,
    };
    fold_config_field(inner, Some(key.to_owned()), &inner_ctx, out, cx);
}

pub(crate) fn fold_config_field<V: 'static>(
    spec: &FormField,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<V>,
) {
    let field = match spec {
        FormField::Text {
            key, placeholder, ..
        } => {
            let flavor = text_flavor(key, &ctx.capabilities);
            let placeholder = if flavor == InputFlavor::VariableName {
                VAR_PLACEHOLDER.into()
            } else {
                field_placeholder(ctx.kind_id, key, placeholder).into()
            };
            build_config_input(plain_input(key, placeholder, flavor), gate, ctx, cx)
        }
        FormField::TextArea { key, .. } => fold_text_area(key, gate, ctx, cx),
        FormField::Code {
            key,
            language,
            kind,
            ..
        } => build_config_code(key, *language, *kind, gate, ctx, cx),
        FormField::Integer { key, min, max, .. } => {
            integer_input(key, InputFlavor::Integer(*min..=*max), gate, ctx, cx)
        }
        FormField::OneBasedInteger { key, count, .. } => {
            integer_input(key, InputFlavor::OneBased(*count), gate, ctx, cx)
        }
        FormField::UnitAmount { key, .. } | FormField::Duration { key, .. } => {
            match AmountScale::of(spec) {
                Some(scale) => build_unit_amount(key, scale, gate, ctx, cx),
                None => return,
            }
        }
        FormField::Slider {
            key,
            min,
            max,
            unit,
            ..
        } => fold_slider(key, *min..=*max, unit, gate, ctx, cx),
        FormField::Swatch { key, options, .. } => fold_swatch(key, options, gate, ctx, cx),
        FormField::Select { .. }
        | FormField::DynamicSelect { .. }
        | FormField::DependentSelect { .. } => match fold_select(spec, gate, ctx, cx) {
            Some(field) => field,
            None => return,
        },
        FormField::FilePicker { key, .. } => build_config_input(
            plain_input(key, SharedString::default(), InputFlavor::File),
            gate,
            ctx,
            cx,
        ),
        FormField::DateTime { key, .. } => build_config_input(
            plain_input(key, SharedString::default(), InputFlavor::DateTime),
            gate,
            ctx,
            cx,
        ),
        FormField::Toggle { key, .. } => ConfigField::Bool {
            key: (*key).to_owned(),
            gate,
            value: matches!(ctx.config.get(*key), Some(Variant::Bool(true))),
            presence_gate: false,
        },
        FormField::SubChain { key, .. } | FormField::CaseList { key, .. } => ConfigField::Hint {
            key: (*key).to_owned(),
        },
        FormField::Optional { key, inner, .. } => {
            fold_optional(key, inner, gate, ctx, out, cx);
            return;
        }
    };
    out.push(field);
}

pub(crate) fn optional_shares_key(key: &str, inner: &FormField) -> bool {
    key == inner.key()
}

pub(crate) fn optional_gate_seed(key: &str, inner: &FormField, config: &FieldConfig) -> bool {
    let stored = config.get(key);
    if optional_shares_key(key, inner) {
        stored.is_some_and(|value| !matches!(value, Variant::Bool(_)))
    } else {
        matches!(stored, Some(Variant::Bool(true)))
    }
}

fn optional_inner_seed<'a>(
    key: &str,
    inner: &FormField,
    config: &'a FieldConfig,
) -> Cow<'a, FieldConfig> {
    if optional_shares_key(key, inner) && matches!(config.get(key), Some(Variant::Bool(_))) {
        let mut seed = config.clone();
        seed.remove(key);
        Cow::Owned(seed)
    } else {
        Cow::Borrowed(config)
    }
}

pub(super) fn read_int(config: &FieldConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(number)) => Some(*number),
        _ => None,
    }
}

pub(crate) fn read_text(config: &FieldConfig, key: &str) -> String {
    config
        .get(key)
        .map(crate::shared::config_text::editable_line)
        .unwrap_or_default()
}

fn variable_name_invalid(text: &str) -> bool {
    !text.trim().is_empty() && normalize_var_name(text).is_none()
}

fn build_config_input<V: 'static>(
    spec: InputSpec<'_>,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let stored = read_text(ctx.config, spec.key);
    let seed = if matches!(spec.flavor, InputFlavor::OneBased(_)) {
        crate::shared::config_text::shown_one_based(&stored)
    } else {
        stored
    };
    let is_variable = spec.flavor == InputFlavor::VariableName;
    let bounds = spec.flavor.shown_bounds();
    let seed_invalid = if is_variable {
        variable_name_invalid(&seed)
    } else {
        bounds
            .as_ref()
            .is_some_and(|range| integer_text_invalid(&seed, range))
    };
    let input = cx.new(|cx| {
        let mut input = TextInput::new(spec.placeholder, cx);
        if is_variable {
            input = input
                .mono()
                .leading_icon(Icon::Variable, |palette| palette.warning)
                .accent(|palette| palette.warning);
        }
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input.set_invalid(seed_invalid, cx);
        input
    });
    let sub = subscribe_input(&input, &spec.flavor, bounds, ctx, cx);
    ConfigField::Input {
        key: spec.key.to_owned(),
        flavor: spec.flavor,
        optional: spec.optional,
        gate,
        input,
        _sub: sub,
    }
}

fn subscribe_input<V: 'static>(
    input: &Entity<TextInput>,
    flavor: &InputFlavor,
    bounds: Option<RangeInclusive<i64>>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> Option<Subscription> {
    match ctx.capabilities.entry {
        EntryPolicy::Inline => {
            let on_committed = ctx.on_committed;
            Some(cx.subscribe(
                input,
                move |view, field: Entity<TextInput>, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Cancelled) {
                        field.update(cx, |input, cx| input.restore_committed(cx));
                        return;
                    }
                    if let Some(range) = &bounds {
                        bound_integer_input(&field, event, range, cx);
                    }
                    on_committed(view, event, cx);
                },
            ))
        }
        EntryPolicy::OnSubmit if *flavor == InputFlavor::VariableName => Some(cx.subscribe(
            input,
            |_, field: Entity<TextInput>, event: &InputEvent, cx| {
                if let InputEvent::Changed(text) = event {
                    let invalid = variable_name_invalid(text);
                    field.update(cx, |input, cx| input.set_invalid(invalid, cx));
                }
            },
        )),
        EntryPolicy::OnSubmit => bounds.map(|range| {
            cx.subscribe(
                input,
                move |_, field: Entity<TextInput>, event: &InputEvent, cx| {
                    flag_changed_integer(&field, event, &range, cx);
                },
            )
        }),
    }
}

pub(super) fn integer_text_invalid(text: &str, range: &RangeInclusive<i64>) -> bool {
    int_entry_invalid(text, *range.start(), *range.end(), false)
}

pub(super) fn clamped_integer_text(text: &str, range: &RangeInclusive<i64>) -> Option<String> {
    let typed = text.trim().parse::<i64>().ok()?;
    let clamped = within(typed, range);
    (clamped != typed).then(|| clamped.to_string())
}

pub(super) fn within(number: i64, range: &RangeInclusive<i64>) -> i64 {
    number.max(*range.start()).min(*range.end())
}

fn flag_changed_integer(
    field: &Entity<TextInput>,
    event: &InputEvent,
    range: &RangeInclusive<i64>,
    cx: &mut App,
) {
    if let InputEvent::Changed(text) = event {
        let invalid = integer_text_invalid(text, range);
        field.update(cx, |input, cx| input.set_invalid(invalid, cx));
    }
}

fn bound_integer_input(
    field: &Entity<TextInput>,
    event: &InputEvent,
    range: &RangeInclusive<i64>,
    cx: &mut App,
) {
    match event {
        InputEvent::Changed(_) => flag_changed_integer(field, event, range, cx),
        InputEvent::Submitted(text) | InputEvent::Blurred(text) => {
            settle_integer_input(field, text, range, cx);
        }
        InputEvent::Cancelled => {}
    }
}

pub(super) fn settle_integer_input(
    field: &Entity<TextInput>,
    text: &str,
    range: &RangeInclusive<i64>,
    cx: &mut App,
) {
    let settled = clamped_integer_text(text, range);
    field.update(cx, |input, cx| {
        if let Some(settled) = settled {
            input.set_content(settled, cx);
        }
        let invalid = integer_text_invalid(input.content(), range);
        input.set_invalid(invalid, cx);
    });
}

pub(crate) fn flag_integer_input(
    field: &Entity<TextInput>,
    range: &RangeInclusive<i64>,
    cx: &mut App,
) -> bool {
    let invalid = integer_text_invalid(field.read(cx).content(), range);
    field.update(cx, |input, cx| input.set_invalid(invalid, cx));
    invalid
}

pub(crate) fn flag_variable_input(field: &Entity<TextInput>, cx: &mut App) -> bool {
    let invalid = variable_name_invalid(field.read(cx).content());
    field.update(cx, |input, cx| input.set_invalid(invalid, cx));
    invalid
}

fn build_unit_amount<V: 'static>(
    key: &str,
    scale: AmountScale,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let AmountSeed { unit, amount: seed } = scale.seed(key, ctx.config);
    let range = scale.range_for(&unit);
    let live_range = Rc::new(Cell::new((*range.start(), *range.end())));
    let seed_invalid = integer_text_invalid(&seed, &range);
    let input = cx.new(|cx| {
        let mut input = TextInput::new(INTEGER_PLACEHOLDER, cx);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input.set_invalid(seed_invalid, cx);
        input
    });
    let on_committed = ctx.on_committed;
    let policy = ctx.capabilities.entry;
    let bound_range = Rc::clone(&live_range);
    let sub = cx.subscribe(
        &input,
        move |view, field: Entity<TextInput>, event: &InputEvent, cx| {
            let (min, max) = bound_range.get();
            let range = min..=max;
            if policy == EntryPolicy::OnSubmit {
                flag_changed_integer(&field, event, &range, cx);
                return;
            }
            if matches!(event, InputEvent::Cancelled) {
                field.update(cx, |input, cx| input.restore_committed(cx));
                return;
            }
            bound_integer_input(&field, event, &range, cx);
            on_committed(view, event, cx);
        },
    );
    ConfigField::UnitAmount {
        key: key.to_owned(),
        unit_picker_key: scale.unit_picker_key(key),
        gate,
        scale,
        units: scale.unit_options(),
        unit,
        live_range,
        policy,
        input,
        _sub: sub,
    }
}

fn build_config_code<V: 'static>(
    key: &str,
    language: CodeLanguage,
    kind: CodeKind,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let seed = ctx
        .config
        .get(key)
        .map(crate::shared::config_text::editable_text)
        .unwrap_or_default();
    let editor = code_field_editor(language, kind, seed, cx);
    let on_committed = ctx.on_committed;
    let sub = (ctx.capabilities.entry == EntryPolicy::Inline).then(|| {
        cx.subscribe(&editor, move |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Blurred(_)) {
                on_committed(view, event, cx);
            }
        })
    });
    ConfigField::Code {
        key: key.to_owned(),
        gate,
        language,
        kind,
        editor,
        _sub: sub,
    }
}

pub(crate) fn code_field_editor<V: 'static>(
    language: CodeLanguage,
    kind: CodeKind,
    seed: String,
    cx: &mut Context<V>,
) -> Entity<CodeEditor> {
    let language = match language {
        CodeLanguage::Rhai => Language::Rhai,
        CodeLanguage::Json => Language::Json,
    };
    cx.new(|cx| {
        let editor = CodeEditor::new(language, "", cx);
        let mut editor = match kind {
            CodeKind::Expression => editor.compact_field(),
            CodeKind::Script => editor.with_field_rows(CODE_FIELD_ROWS),
        };
        if !seed.is_empty() {
            editor.set_content(seed, cx);
        }
        editor
    })
}
