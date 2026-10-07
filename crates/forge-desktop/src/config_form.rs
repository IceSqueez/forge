use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::ops::RangeInclusive;
use std::rc::Rc;

use forge_components::highlight::Language;
use forge_components::{
    BORDER_THIN, CodeEditor, Density, ForgePalette, Icon, InputEvent, Picker, PickerEvent,
    PickerItem, PickerLabels, Radius, Spacing, TextInput, accent_swatch, body_family, dropdown,
    icon, mono_family, radius, slider, spacing, toggle, tr,
};
use forge_registry::{CodeLanguage, FormField, UnitAmountBounds};

use crate::collection_options::{
    ChoiceOptions, CollectionChoiceField, CollectionSource, collection_choice_fields,
};
use crate::donation_services::donation_provider_options;
use crate::obs_catalog_options::{ObsCatalogField, ObsCatalogList};
use crate::presentation::ActivePresentation;
use crate::quick_action_field_rows::int_entry_invalid;
use crate::vtube_catalog_options::{VTubeCatalogField, VTubeCatalogList};
use forge_runtime::triggers::DONATION_PROVIDER_OPTIONS_KEY;
use forge_types::Variant;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};

const FILL_KEY_W: Pixels = px(110.0);
const FILL_KEY_FS: Pixels = px(11.0);
pub(crate) const FILL_VAL_FS: Pixels = px(11.5);
const FILL_ROW_PAD_V: Pixels = px(8.0);
const FILL_ROW_PAD_H: Pixels = px(12.0);

const SLIDER_GAP: Pixels = px(10.0);
const SLIDER_READOUT_W: Pixels = px(34.0);

const SWATCH_GAP: Pixels = px(6.0);
const SWATCH_SIZE: Pixels = px(22.0);
const SWATCH_RADIUS: Pixels = px(6.0);
const SWATCH_RING: Pixels = px(2.0);

pub(crate) const CHOICE_PAD_V: Pixels = px(6.0);
pub(crate) const CHOICE_PAD_H: Pixels = px(9.0);
pub(crate) const CHOICE_GLYPH: Pixels = px(12.0);

const INTEGER_PLACEHOLDER: &str = "0";

const CODE_FIELD_H: Pixels = px(150.0);

type FieldConfig = BTreeMap<String, Variant>;

pub(crate) enum ConfigField {
    Input {
        key: String,
        integer: Option<RangeInclusive<i64>>,
        optional: bool,
        gate: Option<String>,
        input: Entity<TextInput>,
        _sub: Subscription,
    },
    Code {
        key: String,
        gate: Option<String>,
        editor: Entity<CodeEditor>,
        _sub: Subscription,
    },
    Bool {
        key: String,
        gate: Option<String>,
        value: bool,
    },
    Slide {
        key: String,
        gate: Option<String>,
        min: i64,
        max: i64,
        unit: &'static str,
        value: i64,
    },
    Swatch {
        key: String,
        gate: Option<String>,
        options: Vec<(String, gpui::Rgba)>,
        selected: String,
    },
    Choice {
        key: String,
        gate: Option<String>,
        options: Vec<(String, String)>,
        selected: String,
        dependency: Option<ChoiceDependency>,
    },
    UnitAmount {
        key: String,
        unit_key: String,
        gate: Option<String>,
        bounds: UnitAmountBounds,
        units: Vec<(String, String)>,
        unit: String,
        max: Rc<Cell<i64>>,
        input: Entity<TextInput>,
        _sub: Subscription,
    },
    Hint {
        key: String,
    },
}

pub(crate) struct ChoiceDependency {
    options_prefix: String,
    depends_on: String,
}

impl ConfigField {
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Input { key, .. }
            | Self::Code { key, .. }
            | Self::Bool { key, .. }
            | Self::Slide { key, .. }
            | Self::Swatch { key, .. }
            | Self::Choice { key, .. }
            | Self::UnitAmount { key, .. }
            | Self::Hint { key } => key,
        }
    }
}

pub(crate) type ConfigCommitHandler<V> = fn(&mut V, &InputEvent, &mut Context<V>);

type ChoiceOpener<V> = fn(&mut V, String, &mut Window, &mut Context<V>);

type ChoiceCloser<V> = fn(&mut V, &mut Context<V>);

pub(crate) struct ChoiceDropdown<V: 'static> {
    pub(crate) open: ChoiceOpener<V>,
    pub(crate) close: ChoiceCloser<V>,
    pub(crate) active: Option<(String, Entity<Picker>)>,
}

pub(crate) enum ChoiceSupport<'a> {
    Text,
    Picker(&'a HashMap<String, Vec<(String, String)>>),
}

pub(crate) struct ConfigFieldHandlers<V: 'static> {
    pub(crate) toggle: fn(&mut V, String, &mut Context<V>),
    pub(crate) slide: fn(&mut V, String, i64, &mut Context<V>),
    pub(crate) pick: fn(&mut V, String, String, &mut Context<V>),
    pub(crate) choice: Option<ChoiceDropdown<V>>,
}

pub(crate) fn sparse_overrides(default: &FieldConfig, buffer: &FieldConfig) -> FieldConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub(crate) struct FoldContext<'a, V: 'static> {
    pub(crate) config: &'a FieldConfig,
    pub(crate) defaults: &'a FieldConfig,
    pub(crate) palette: &'a ForgePalette,
    pub(crate) choices: ChoiceSupport<'a>,
    pub(crate) on_committed: ConfigCommitHandler<V>,
}

struct InputSpec<'a> {
    key: &'a str,
    placeholder: SharedString,
    integer: Option<RangeInclusive<i64>>,
    optional: bool,
}

fn free_text(key: &str, placeholder: SharedString) -> InputSpec<'_> {
    InputSpec {
        key,
        placeholder,
        integer: None,
        optional: false,
    }
}

pub(crate) fn fold_config_field<V: 'static>(
    spec: &FormField,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<V>,
) {
    let choices = &ctx.choices;
    match spec {
        FormField::Text {
            key, placeholder, ..
        } => out.push(build_config_input(
            free_text(key, (*placeholder).into()),
            gate,
            ctx,
            cx,
        )),
        FormField::TextArea { key, .. } => out.push(build_config_input(
            free_text(key, SharedString::default()),
            gate,
            ctx,
            cx,
        )),
        FormField::Code { key, language, .. } => {
            out.push(build_config_code(key, *language, gate, ctx, cx))
        }
        FormField::Integer { key, min, max, .. } => {
            let optional = !ctx.defaults.contains_key(*key);
            let placeholder = if optional {
                SharedString::from(tr!("config_form_auto_placeholder"))
            } else {
                SharedString::from(INTEGER_PLACEHOLDER)
            };
            out.push(build_config_input(
                InputSpec {
                    key,
                    placeholder,
                    integer: Some(*min..=*max),
                    optional,
                },
                gate,
                ctx,
                cx,
            ));
        }
        FormField::UnitAmount {
            key,
            unit_key,
            bounds,
            ..
        } => out.push(build_unit_amount(key, unit_key, *bounds, gate, ctx, cx)),
        FormField::Slider {
            key,
            min,
            max,
            unit,
            ..
        } => out.push(ConfigField::Slide {
            key: (*key).to_owned(),
            gate,
            min: *min,
            max: *max,
            unit,
            value: read_int(ctx.config, key).unwrap_or(*min).clamp(*min, *max),
        }),
        FormField::Swatch { key, options, .. } => out.push(ConfigField::Swatch {
            key: (*key).to_owned(),
            gate,
            options: options
                .iter()
                .map(|name| {
                    let tint = accent_swatch(name, ctx.palette).unwrap_or(ctx.palette.text_faint);
                    ((*name).to_owned(), tint)
                })
                .collect(),
            selected: read_text(ctx.config, key),
        }),
        FormField::Select { key, options, .. } => match choices {
            ChoiceSupport::Text => out.push(build_config_input(
                free_text(key, SharedString::default()),
                gate,
                ctx,
                cx,
            )),
            ChoiceSupport::Picker(_) => out.push(ConfigField::Choice {
                key: (*key).to_owned(),
                gate,
                options: options
                    .iter()
                    .map(|opt| {
                        let label = crate::motion_labels::preset_label(key, opt);
                        ((*opt).to_owned(), label)
                    })
                    .collect(),
                selected: read_text(ctx.config, key),
                dependency: None,
            }),
        },
        FormField::DynamicSelect {
            key, options_key, ..
        } => match choices {
            ChoiceSupport::Picker(map) => out.push(ConfigField::Choice {
                key: (*key).to_owned(),
                gate,
                options: map.get(*options_key).cloned().unwrap_or_default(),
                selected: read_text(ctx.config, key),
                dependency: None,
            }),
            ChoiceSupport::Text if has_options_provider(options_key) => {
                out.push(ConfigField::Choice {
                    key: (*key).to_owned(),
                    gate,
                    options: Vec::new(),
                    selected: read_text(ctx.config, key),
                    dependency: None,
                })
            }
            ChoiceSupport::Text => out.push(build_config_input(
                free_text(key, SharedString::default()),
                gate,
                ctx,
                cx,
            )),
        },
        FormField::DependentSelect {
            key,
            options_prefix,
            depends_on,
            ..
        } => match choices {
            ChoiceSupport::Text => out.push(build_config_input(
                free_text(key, SharedString::default()),
                gate,
                ctx,
                cx,
            )),
            ChoiceSupport::Picker(map) => out.push(ConfigField::Choice {
                key: (*key).to_owned(),
                gate,
                options: dependent_options(map, options_prefix, &read_text(ctx.config, depends_on)),
                selected: read_text(ctx.config, key),
                dependency: Some(ChoiceDependency {
                    options_prefix: (*options_prefix).to_owned(),
                    depends_on: (*depends_on).to_owned(),
                }),
            }),
        },
        FormField::FilePicker { key, .. } | FormField::DateTime { key, .. } => out.push(
            build_config_input(free_text(key, SharedString::default()), gate, ctx, cx),
        ),
        FormField::Toggle { key, .. } => {
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate,
                value: matches!(ctx.config.get(*key), Some(Variant::Bool(true))),
            });
        }
        FormField::SubChain { key, .. } | FormField::CaseList { key, .. } => {
            out.push(ConfigField::Hint {
                key: (*key).to_owned(),
            });
        }
        FormField::Optional { key, inner, .. } => {
            let value = matches!(ctx.config.get(*key), Some(Variant::Bool(true)));
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate: gate.clone(),
                value,
            });
            fold_config_field(inner, Some((*key).to_owned()), ctx, out, cx);
        }
    }
}

pub(crate) fn set_picked_value(fields: &mut [ConfigField], key: &str, value: &str, cx: &mut App) {
    for field in fields {
        match field {
            ConfigField::Swatch {
                key: k, selected, ..
            }
            | ConfigField::Choice {
                key: k, selected, ..
            } if k == key => value.clone_into(selected),
            ConfigField::UnitAmount {
                unit_key,
                bounds,
                unit,
                max,
                input,
                ..
            } if unit_key == key => {
                value.clone_into(unit);
                let range = bounds.range_for(unit);
                max.set(*range.end());
                let content = input.read(cx).content().to_owned();
                settle_integer_input(input, &content, &range, cx);
            }
            _ => {}
        }
    }
}

pub(crate) fn choice_entries<'a>(
    fields: &'a [ConfigField],
    key: &str,
) -> Option<(&'a [(String, String)], &'a str)> {
    fields.iter().find_map(|field| match field {
        ConfigField::Choice {
            key: k,
            options,
            selected,
            ..
        } if k == key => Some((options.as_slice(), selected.as_str())),
        ConfigField::UnitAmount {
            unit_key,
            units,
            unit,
            ..
        } if unit_key == key => Some((units.as_slice(), unit.as_str())),
        _ => None,
    })
}

struct ChoicePopover {
    key: String,
    picker: Entity<Picker>,
    _sub: Subscription,
}

fn static_choice_options(options_key: &str) -> Option<Vec<(String, String)>> {
    (options_key == DONATION_PROVIDER_OPTIONS_KEY).then(donation_provider_options)
}

fn has_options_provider(options_key: &str) -> bool {
    static_choice_options(options_key).is_some()
        || CollectionSource::parse(options_key).is_some()
        || ObsCatalogList::parse(options_key).is_some()
        || VTubeCatalogList::parse(options_key).is_some()
}

struct ChoiceOptionsKey {
    field_key: String,
    options_key: String,
}

fn choice_options_keys(specs: &[FormField]) -> Vec<ChoiceOptionsKey> {
    let mut out = Vec::new();
    for spec in specs {
        push_choice_options_key(spec, &mut out);
    }
    out
}

fn push_choice_options_key(spec: &FormField, out: &mut Vec<ChoiceOptionsKey>) {
    match spec {
        FormField::DynamicSelect {
            key, options_key, ..
        } if has_options_provider(options_key) => out.push(ChoiceOptionsKey {
            field_key: (*key).to_owned(),
            options_key: (*options_key).to_owned(),
        }),
        FormField::Optional { inner, .. } => push_choice_options_key(inner, out),
        _ => {}
    }
}

#[derive(Default)]
pub(crate) struct CollectionChoices {
    fields: Vec<CollectionChoiceField>,
    options_keys: Vec<ChoiceOptionsKey>,
    popover: Option<ChoicePopover>,
}

impl CollectionChoices {
    pub(crate) fn for_specs(specs: &[FormField]) -> Self {
        Self {
            fields: collection_choice_fields(specs),
            options_keys: choice_options_keys(specs),
            popover: None,
        }
    }

    pub(crate) fn fields(&self) -> &[CollectionChoiceField] {
        &self.fields
    }

    pub(crate) fn obs_fields(&self) -> Vec<ObsCatalogField> {
        self.options_keys
            .iter()
            .filter_map(|keys| {
                ObsCatalogList::parse(&keys.options_key).map(|list| ObsCatalogField {
                    options_key: keys.options_key.clone(),
                    list,
                })
            })
            .collect()
    }

    pub(crate) fn vtube_fields(&self) -> Vec<VTubeCatalogField> {
        self.options_keys
            .iter()
            .filter_map(|keys| {
                VTubeCatalogList::parse(&keys.options_key).map(|list| VTubeCatalogField {
                    options_key: keys.options_key.clone(),
                    list,
                })
            })
            .collect()
    }

    pub(crate) fn refresh(
        &self,
        config_fields: &mut [ConfigField],
        options: &ChoiceOptions,
        blank: Option<&str>,
    ) {
        for field in config_fields {
            let ConfigField::Choice {
                key,
                options: offered,
                ..
            } = field
            else {
                continue;
            };
            let Some(choice_field) = self.options_keys.iter().find(|f| f.field_key == *key) else {
                continue;
            };
            let mut next: Vec<(String, String)> = blank
                .map(|label| (String::new(), label.to_owned()))
                .into_iter()
                .collect();
            next.extend(
                options
                    .get(&choice_field.options_key)
                    .cloned()
                    .or_else(|| static_choice_options(&choice_field.options_key))
                    .unwrap_or_default(),
            );
            *offered = next;
        }
    }

    pub(crate) fn toggle<V: 'static>(
        &mut self,
        config_fields: &[ConfigField],
        key: String,
        on_event: fn(&mut V, Entity<Picker>, &PickerEvent, &mut Context<V>),
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        if self.popover.as_ref().is_some_and(|open| open.key == key) {
            self.popover = None;
            return;
        }
        self.popover = open_choice_popover(config_fields, key, on_event, window, cx);
    }

    pub(crate) fn close(&mut self) {
        self.popover = None;
    }

    pub(crate) fn take_open_key(&mut self) -> Option<String> {
        self.popover.take().map(|open| open.key)
    }

    pub(crate) fn active(&self) -> Option<(String, Entity<Picker>)> {
        self.popover
            .as_ref()
            .map(|open| (open.key.clone(), open.picker.clone()))
    }
}

fn open_choice_popover<V: 'static>(
    fields: &[ConfigField],
    key: String,
    on_event: fn(&mut V, Entity<Picker>, &PickerEvent, &mut Context<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<ChoicePopover> {
    let (options, selected) = choice_entries(fields, &key)?;
    let items: Vec<PickerItem> = options
        .iter()
        .map(|(value, label)| PickerItem {
            id: SharedString::from(value.clone()),
            label: SharedString::from(label.clone()),
            sublabel: None,
            icon: None,
        })
        .collect();
    let labels = PickerLabels {
        placeholder: tr!("config_form_choice_search").into(),
        empty: tr!("config_form_choice_empty").into(),
        loading: tr!("widget_picker_loading").into(),
    };
    let current = Some(SharedString::from(selected.to_owned()));
    let palette = cx.palette();
    let picker = cx.new(|cx| {
        Picker::new(labels, items, palette, cx)
            .with_current(current)
            .with_custom_entry(tr!("config_form_choice_custom").into())
    });
    let sub = cx.subscribe(&picker, on_event);
    picker.update(cx, |picker, cx| picker.focus(window, cx));
    Some(ChoicePopover {
        key,
        picker,
        _sub: sub,
    })
}

pub(crate) fn dependent_options(
    map: &HashMap<String, Vec<(String, String)>>,
    options_prefix: &str,
    sibling_value: &str,
) -> Vec<(String, String)> {
    if sibling_value.is_empty() {
        return Vec::new();
    }
    map.get(&format!("{options_prefix}.{sibling_value}"))
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn resolve_dependent_choices(
    fields: &mut [ConfigField],
    map: &HashMap<String, Vec<(String, String)>>,
    cx: &App,
) {
    let mut values = FieldConfig::new();
    collect_field_values(fields, &mut values, cx);
    for field in fields.iter_mut() {
        if let ConfigField::Choice {
            options,
            dependency: Some(dependency),
            ..
        } = field
        {
            *options = dependent_options(
                map,
                &dependency.options_prefix,
                &read_text(&values, &dependency.depends_on),
            );
        }
    }
}

fn read_int(config: &FieldConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(number)) => Some(*number),
        _ => None,
    }
}

fn read_text(config: &FieldConfig, key: &str) -> String {
    config
        .get(key)
        .map(forge_types::display_scalar)
        .unwrap_or_default()
}

fn build_config_input<V: 'static>(
    spec: InputSpec<'_>,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let seed = read_text(ctx.config, spec.key);
    let palette = *ctx.palette;
    let seed_invalid = spec
        .integer
        .as_ref()
        .is_some_and(|range| integer_text_invalid(&seed, range));
    let input = cx.new(|cx| {
        let mut input = TextInput::new(spec.placeholder, cx).with_palette(palette);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input.set_invalid(seed_invalid, cx);
        input
    });
    let on_committed = ctx.on_committed;
    let range = spec.integer.clone();
    let sub = cx.subscribe(
        &input,
        move |view, field: Entity<TextInput>, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Cancelled) {
                field.update(cx, |input, cx| input.restore_committed(cx));
                return;
            }
            if let Some(range) = &range {
                bound_integer_input(&field, event, range, cx);
            }
            on_committed(view, event, cx);
        },
    );
    ConfigField::Input {
        key: spec.key.to_owned(),
        integer: spec.integer,
        optional: spec.optional,
        gate,
        input,
        _sub: sub,
    }
}

fn integer_text_invalid(text: &str, range: &RangeInclusive<i64>) -> bool {
    int_entry_invalid(text, *range.start(), *range.end(), false)
}

fn clamped_integer_text(text: &str, range: &RangeInclusive<i64>) -> Option<String> {
    let typed = text.trim().parse::<i64>().ok()?;
    let clamped = within(typed, range);
    (clamped != typed).then(|| clamped.to_string())
}

fn within(number: i64, range: &RangeInclusive<i64>) -> i64 {
    number.max(*range.start()).min(*range.end())
}

fn bound_integer_input(
    field: &Entity<TextInput>,
    event: &InputEvent,
    range: &RangeInclusive<i64>,
    cx: &mut App,
) {
    match event {
        InputEvent::Changed(text) => {
            let invalid = integer_text_invalid(text, range);
            field.update(cx, |input, cx| input.set_invalid(invalid, cx));
        }
        InputEvent::Submitted(text) | InputEvent::Blurred(text) => {
            settle_integer_input(field, text, range, cx);
        }
        InputEvent::Cancelled => {}
    }
}

fn settle_integer_input(
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

fn build_unit_amount<V: 'static>(
    key: &str,
    unit_key: &str,
    bounds: UnitAmountBounds,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let unit = bounds
        .unit_or_first(&read_text(ctx.config, unit_key))
        .map(|unit| unit.value.to_owned())
        .unwrap_or_default();
    let range = bounds.range_for(&unit);
    let max = Rc::new(Cell::new(*range.end()));
    let seed = read_text(ctx.config, key);
    let seed_invalid = integer_text_invalid(&seed, &range);
    let palette = *ctx.palette;
    let input = cx.new(|cx| {
        let mut input = TextInput::new(INTEGER_PLACEHOLDER, cx).with_palette(palette);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input.set_invalid(seed_invalid, cx);
        input
    });
    let on_committed = ctx.on_committed;
    let min = bounds.min;
    let live_max = Rc::clone(&max);
    let sub = cx.subscribe(
        &input,
        move |view, field: Entity<TextInput>, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Cancelled) {
                field.update(cx, |input, cx| input.restore_committed(cx));
                return;
            }
            bound_integer_input(&field, event, &(min..=live_max.get()), cx);
            on_committed(view, event, cx);
        },
    );
    let units = bounds
        .units
        .iter()
        .map(|unit| {
            let label = crate::motion_labels::preset_label(unit_key, unit.value);
            (unit.value.to_owned(), label)
        })
        .collect();
    ConfigField::UnitAmount {
        key: key.to_owned(),
        unit_key: unit_key.to_owned(),
        gate,
        bounds,
        units,
        unit,
        max,
        input,
        _sub: sub,
    }
}

fn build_config_code<V: 'static>(
    key: &str,
    language: CodeLanguage,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let editor = code_field_editor(
        language,
        read_text(ctx.config, key),
        CODE_FIELD_H,
        *ctx.palette,
        cx,
    );
    let on_committed = ctx.on_committed;
    let sub = cx.subscribe(&editor, move |view, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Blurred(_)) {
            on_committed(view, event, cx);
        }
    });
    ConfigField::Code {
        key: key.to_owned(),
        gate,
        editor,
        _sub: sub,
    }
}

pub(crate) fn code_field_editor<V: 'static>(
    language: CodeLanguage,
    seed: String,
    height: Pixels,
    palette: ForgePalette,
    cx: &mut Context<V>,
) -> Entity<CodeEditor> {
    let language = match language {
        CodeLanguage::Rhai => Language::Rhai,
        CodeLanguage::Json => Language::Json,
    };
    cx.new(|cx| {
        let mut editor = CodeEditor::new(language, "", cx)
            .with_palette(palette)
            .with_field_height(height);
        if !seed.is_empty() {
            editor.set_content(seed, cx);
        }
        editor
    })
}

pub(crate) fn collect_field_values(fields: &[ConfigField], buffer: &mut FieldConfig, cx: &App) {
    let bool_vals: HashMap<&str, bool> = fields
        .iter()
        .filter_map(|f| match f {
            ConfigField::Bool { key, value, .. } => Some((key.as_str(), *value)),
            _ => None,
        })
        .collect();
    let gate_on = |gate: &Option<String>| {
        gate.as_ref()
            .map(|g| bool_vals.get(g.as_str()).copied().unwrap_or(false))
            .unwrap_or(true)
    };

    for field in fields {
        match field {
            ConfigField::Bool {
                key, value, gate, ..
            } => {
                if gate_on(gate) {
                    buffer.insert(key.clone(), Variant::Bool(*value));
                }
            }
            ConfigField::Slide {
                key, gate, value, ..
            } => {
                if gate_on(gate) {
                    buffer.insert(key.clone(), Variant::Int(*value));
                }
            }
            ConfigField::Swatch {
                key,
                gate,
                selected,
                ..
            } => {
                if gate_on(gate) && !selected.is_empty() {
                    buffer.insert(key.clone(), Variant::String(selected.clone()));
                }
            }
            ConfigField::Choice {
                key,
                gate,
                selected,
                ..
            } => {
                if gate_on(gate) {
                    buffer.insert(key.clone(), Variant::String(selected.clone()));
                }
            }
            ConfigField::Code {
                key, gate, editor, ..
            } => {
                if gate_on(gate) {
                    let text = editor.read(cx).content().to_owned();
                    buffer.insert(key.clone(), Variant::String(text));
                }
            }
            ConfigField::Input {
                key,
                integer,
                optional,
                gate,
                input,
                ..
            } => {
                if !gate_on(gate) {
                    continue;
                }
                let text = input.read(cx).content().to_owned();
                let Some(range) = integer else {
                    buffer.insert(key.clone(), Variant::String(text));
                    continue;
                };
                let typed = text.trim();
                if let Ok(number) = typed.parse::<i64>() {
                    let bounded = within(number, range);
                    buffer.insert(key.clone(), Variant::Int(bounded));
                } else if *optional && typed.is_empty() {
                    buffer.remove(key);
                }
            }
            ConfigField::UnitAmount {
                key,
                unit_key,
                gate,
                bounds,
                unit,
                input,
                ..
            } => {
                if !gate_on(gate) {
                    continue;
                }
                buffer.insert(unit_key.clone(), Variant::String(unit.clone()));
                if let Ok(number) = input.read(cx).content().trim().parse::<i64>() {
                    let bounded = within(number, &bounds.range_for(unit));
                    buffer.insert(key.clone(), Variant::Int(bounded));
                }
            }
            ConfigField::Hint { .. } => {}
        }
    }
}

pub(crate) fn render_config_control<V: 'static>(
    field: &ConfigField,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
) -> AnyElement {
    match field {
        ConfigField::Input { input, .. } => div().child(input.clone()).into_any_element(),
        ConfigField::Code { editor, .. } => div().child(editor.clone()).into_any_element(),
        ConfigField::Bool { key, value, .. } => {
            let toggle_key = key.clone();
            let view = view.clone();
            let on_toggle = handlers.toggle;
            toggle(*value, palette)
                .on_click(
                    SharedString::from(format!("{id_prefix}-{key}")),
                    move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                        view.update(cx, |this, cx| on_toggle(this, toggle_key.clone(), cx));
                    },
                )
                .into_any_element()
        }
        ConfigField::Slide {
            key,
            min,
            max,
            unit,
            value,
            ..
        } => render_slide(
            key, *min, *max, unit, *value, palette, id_prefix, view, handlers,
        ),
        ConfigField::Swatch {
            key,
            options,
            selected,
            ..
        } => render_swatch(key, options, selected, palette, id_prefix, view, handlers),
        ConfigField::Choice {
            key,
            options,
            selected,
            ..
        } => render_choice(key, options, selected, palette, id_prefix, view, handlers),
        ConfigField::UnitAmount {
            unit_key,
            units,
            unit,
            input,
            ..
        } => div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(div().flex_1().min_w(px(0.0)).child(input.clone()))
            .child(div().flex_1().min_w(px(0.0)).child(render_choice(
                unit_key, units, unit, palette, id_prefix, view, handlers,
            )))
            .into_any_element(),
        ConfigField::Hint { .. } => div()
            .italic()
            .font_family(body_family())
            .text_size(FILL_VAL_FS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_config_authored"))
            .into_any_element(),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_slide<V: 'static>(
    key: &str,
    min: i64,
    max: i64,
    unit: &str,
    value: i64,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
) -> AnyElement {
    let slide_key = key.to_owned();
    let view = view.clone();
    let on_slide = handlers.slide;

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(SLIDER_GAP)
        .child(
            div().flex_1().min_w(px(0.0)).child(
                slider(value as f32, min as f32, max as f32, palette)
                    .accent(palette.brand)
                    .on_change(
                        SharedString::from(format!("{id_prefix}-{key}")),
                        move |next: &f32, _window: &mut Window, cx: &mut App| {
                            let stepped = next.round() as i64;
                            view.update(cx, |this, cx| {
                                on_slide(this, slide_key.clone(), stepped, cx)
                            });
                        },
                    ),
            ),
        )
        .child(
            div()
                .flex_none()
                .min_w(SLIDER_READOUT_W)
                .whitespace_nowrap()
                .font_family(mono_family())
                .text_size(FILL_VAL_FS)
                .text_color(palette.text_secondary)
                .child(format!("{value}{unit}")),
        )
        .into_any_element()
}

fn render_swatch<V: 'static>(
    key: &str,
    options: &[(String, gpui::Rgba)],
    selected: &str,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
) -> AnyElement {
    let mut row = div().flex().flex_row().flex_wrap().gap(SWATCH_GAP);

    for (name, tint) in options {
        let ring = if name == selected {
            palette.text_primary
        } else {
            palette.surface_overlay
        };
        let pick_key = key.to_owned();
        let pick_value = name.clone();
        let view = view.clone();
        let on_pick = handlers.pick;

        row = row.child(
            div()
                .id(SharedString::from(format!("{id_prefix}-{key}-{name}")))
                .size(SWATCH_SIZE)
                .rounded(SWATCH_RADIUS)
                .bg(*tint)
                .border(SWATCH_RING)
                .border_color(ring)
                .cursor_pointer()
                .on_click(move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                    view.update(cx, |this, cx| {
                        on_pick(this, pick_key.clone(), pick_value.clone(), cx)
                    });
                }),
        );
    }

    row.into_any_element()
}

fn render_choice<V: 'static>(
    key: &str,
    options: &[(String, String)],
    selected: &str,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
) -> AnyElement {
    let (display, tone) = match options.iter().find(|(value, _)| value == selected) {
        Some((_, label)) => (label.clone(), palette.text_primary),
        None if !selected.is_empty() => (selected.to_owned(), palette.text_primary),
        None => (tr!("config_form_choice_placeholder"), palette.text_faint),
    };

    let mut trigger = div()
        .id(SharedString::from(format!("{id_prefix}-{key}")))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(CHOICE_PAD_V)
        .px(CHOICE_PAD_H)
        .rounded(radius(Radius::Sm))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .font_family(body_family())
                .text_size(FILL_VAL_FS)
                .text_color(tone)
                .child(display),
        )
        .child(icon(Icon::ChevronDown, CHOICE_GLYPH, palette.text_faint));

    let Some(choice) = handlers.choice.as_ref() else {
        return trigger.into_any_element();
    };
    let open_choice = choice.open;
    let open_key = key.to_owned();
    let open_view = view.clone();
    let hover_border = palette.brand;
    trigger = trigger
        .cursor_pointer()
        .hover(move |s| s.border_color(hover_border))
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            open_view.update(cx, |this, cx| {
                open_choice(this, open_key.clone(), window, cx)
            });
        });

    let close_choice = choice.close;
    let popover = choice
        .active
        .as_ref()
        .filter(|(active_key, _)| active_key == key)
        .map(|(_, picker)| {
            let view = view.clone();
            dropdown(picker.clone()).on_dismiss(move |_window, cx| {
                view.update(cx, close_choice);
            })
        });

    div()
        .relative()
        .w_full()
        .child(trigger)
        .children(popover)
        .into_any_element()
}

pub(crate) fn render_config_row<V: 'static>(
    field: &ConfigField,
    label: SharedString,
    last: bool,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
) -> AnyElement {
    let label = config_label_cell(label, FILL_KEY_W, FILL_KEY_FS, palette.text_muted);

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, Density::Cozy))
        .py(FILL_ROW_PAD_V)
        .px(FILL_ROW_PAD_H)
        .when(!last, |row| {
            row.border_b(BORDER_THIN)
                .border_color(palette.border_regular)
        })
        .child(label)
        .child(div().flex_1().min_w(px(0.0)).child(render_config_control(
            field, palette, id_prefix, view, handlers,
        )))
        .into_any_element()
}

pub(crate) fn config_label_cell(
    label: SharedString,
    width: Pixels,
    size: Pixels,
    color: gpui::Rgba,
) -> gpui::Div {
    div()
        .w(width)
        .min_w(width)
        .max_w(width)
        .flex_none()
        .whitespace_normal()
        .font_family(mono_family())
        .text_size(size)
        .text_color(color)
        .child(label)
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    fn config(entries: &[(&str, Variant)]) -> FieldConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    #[test]
    fn a_value_equal_to_the_kind_default_is_not_stored_as_an_override() {
        let default = config(&[
            ("accent", Variant::String("mauve".into())),
            ("duration", Variant::Int(5)),
            ("sticky", Variant::Bool(false)),
        ]);
        let buffer = config(&[
            ("accent", Variant::String("mauve".into())),
            ("duration", Variant::Int(9)),
            ("sticky", Variant::Bool(false)),
            ("headline", Variant::String("%user%".into())),
        ]);

        assert_eq!(
            sparse_overrides(&default, &buffer),
            config(&[
                ("duration", Variant::Int(9)),
                ("headline", Variant::String("%user%".into())),
            ]),
            "a record must carry only what the user moved away from the kind default"
        );
    }

    #[test]
    fn a_value_that_only_looks_like_the_default_is_still_an_override() {
        let default = config(&[("duration", Variant::Int(5))]);
        let buffer = config(&[("duration", Variant::String("5".into()))]);

        assert_eq!(
            sparse_overrides(&default, &buffer),
            buffer,
            "a value stored under a different type is not the default, however it displays"
        );
    }

    #[test]
    fn a_default_the_buffer_never_mentions_is_not_resurrected_as_an_override() {
        let default = config(&[
            ("accent", Variant::String("mauve".into())),
            ("duration", Variant::Int(5)),
        ]);

        assert!(
            sparse_overrides(&default, &FieldConfig::new()).is_empty(),
            "an empty form means no overrides, not every default written out longhand"
        );
    }

    fn options_map(entries: &[(&str, &[(&str, &str)])]) -> HashMap<String, Vec<(String, String)>> {
        entries
            .iter()
            .map(|(key, options)| {
                (
                    (*key).to_owned(),
                    options
                        .iter()
                        .map(|(value, label)| ((*value).to_owned(), (*label).to_owned()))
                        .collect(),
                )
            })
            .collect()
    }

    fn voice_options() -> HashMap<String, Vec<(String, String)>> {
        options_map(&[
            ("tts.voices", &[("bare", "Bare prefix")]),
            ("tts.voices.", &[("empty-engine", "Empty engine")]),
            ("tts.voicespiper", &[("glued", "Glued key")]),
            ("tts.voices.piper", &[("amy", "Amy"), ("alan", "Alan")]),
            ("tts.voices.azure", &[("aria", "Aria")]),
        ])
    }

    #[test]
    fn a_dependent_list_is_read_from_the_prefix_joined_to_the_sibling_by_a_dot() {
        assert_eq!(
            dependent_options(&voice_options(), "tts.voices", "piper"),
            vec![
                ("amy".to_owned(), "Amy".to_owned()),
                ("alan".to_owned(), "Alan".to_owned()),
            ],
            "the chosen engine selects its own voices, not the bare prefix and not a glued key"
        );
    }

    #[test]
    fn a_dependent_list_stays_empty_while_the_sibling_names_nothing_known() {
        for sibling in ["", "espeak"] {
            assert!(
                dependent_options(&voice_options(), "tts.voices", sibling).is_empty(),
                "engine {sibling:?} has no registered voices, so the dropdown must offer none"
            );
        }
    }

    #[gpui::test]
    fn clearing_a_choice_erases_the_stored_value_while_clearing_a_swatch_keeps_it(
        cx: &mut gpui::TestAppContext,
    ) {
        let seeded = config(&[
            ("sound", Variant::String("clip:01J9P4S2M7".into())),
            ("accent", Variant::String("mauve".into())),
        ]);
        let cleared = vec![
            choice("sound", "", None),
            ConfigField::Swatch {
                key: "accent".to_owned(),
                gate: None,
                options: Vec::new(),
                selected: String::new(),
            },
        ];

        assert_eq!(
            collected(cx, &cleared, &seeded),
            config(&[
                ("sound", Variant::String(String::new())),
                ("accent", Variant::String("mauve".into())),
            ]),
            "picking the empty option must unset the value, while a swatch with nothing selected keeps it"
        );
    }

    fn choice(key: &str, selected: &str, depends_on: Option<&str>) -> ConfigField {
        ConfigField::Choice {
            key: key.to_owned(),
            gate: None,
            options: Vec::new(),
            selected: selected.to_owned(),
            dependency: depends_on.map(|depends_on| ChoiceDependency {
                options_prefix: "tts.voices".to_owned(),
                depends_on: depends_on.to_owned(),
            }),
        }
    }

    fn offered_values(field: &ConfigField) -> Vec<String> {
        match field {
            ConfigField::Choice { options, .. } => {
                options.iter().map(|(value, _)| value.clone()).collect()
            }
            _ => panic!("expected a choice field"),
        }
    }

    fn selected_value(field: &ConfigField) -> String {
        match field {
            ConfigField::Choice { selected, .. } => selected.clone(),
            _ => panic!("expected a choice field"),
        }
    }

    fn set_selected(field: &mut ConfigField, value: &str) {
        match field {
            ConfigField::Choice { selected, .. } => value.clone_into(selected),
            _ => panic!("expected a choice field"),
        }
    }

    #[gpui::test]
    fn a_dependent_choice_reoffers_its_list_when_the_sibling_moves(cx: &mut gpui::TestAppContext) {
        let map = voice_options();
        let mut fields = vec![
            choice("engine_id", "piper", None),
            choice("voice_id", "", Some("engine_id")),
        ];

        cx.update(|cx| {
            resolve_dependent_choices(&mut fields, &map, cx);
            assert_eq!(
                offered_values(&fields[1]),
                ["amy", "alan"],
                "the voice list must follow the engine the form currently holds"
            );

            set_selected(&mut fields[0], "azure");
            resolve_dependent_choices(&mut fields, &map, cx);
            assert_eq!(
                offered_values(&fields[1]),
                ["aria"],
                "switching engines must re-offer the new engine's voices"
            );
        });
    }

    #[gpui::test]
    fn a_selection_the_new_sibling_no_longer_offers_is_left_standing(
        cx: &mut gpui::TestAppContext,
    ) {
        let map = voice_options();
        let mut fields = vec![
            choice("engine_id", "azure", None),
            choice("voice_id", "amy", Some("engine_id")),
        ];

        cx.update(|cx| {
            resolve_dependent_choices(&mut fields, &map, cx);

            assert_eq!(
                selected_value(&fields[1]),
                "amy",
                "reoffering options must not silently rewrite or clear what the user picked"
            );
        });
    }

    const NOTE_KEY: &str = "note";
    const SEEDED: i64 = 9;

    struct Host;

    fn ignore_commit(_: &mut Host, _: &InputEvent, _: &mut Context<Host>) {}

    fn integer_spec(key: &'static str) -> FormField {
        FormField::Integer {
            key,
            label: "Note",
            min: 0,
            max: 127,
        }
    }

    fn optional_integer_spec(key: &'static str) -> FormField {
        FormField::Optional {
            key,
            label: "Note",
            inner: Box::new(integer_spec(key)),
        }
    }

    fn build(
        cx: &mut gpui::TestAppContext,
        spec: &FormField,
        defaults: &FieldConfig,
        config: &FieldConfig,
        typed: &str,
    ) -> (gpui::Entity<Host>, Vec<ConfigField>) {
        let host = cx.update(|cx| cx.new(|_| Host));
        let fields = host.update(cx, |_, cx| {
            let palette = forge_components::ThemeId::ForgeDefault.palette();
            let ctx = FoldContext {
                config,
                defaults,
                palette: &palette,
                choices: ChoiceSupport::Text,
                on_committed: ignore_commit as ConfigCommitHandler<Host>,
            };
            let mut fields = Vec::new();
            fold_config_field(spec, None, &ctx, &mut fields, cx);
            for field in &fields {
                if let ConfigField::Input { input, .. } = field {
                    input.update(cx, |field, cx| field.set_content(typed.to_owned(), cx));
                }
            }
            fields
        });
        (host, fields)
    }

    fn collected(
        cx: &mut gpui::TestAppContext,
        fields: &[ConfigField],
        seed: &FieldConfig,
    ) -> FieldConfig {
        let mut buffer = seed.clone();
        cx.update(|cx| collect_field_values(fields, &mut buffer, cx));
        buffer
    }

    #[gpui::test]
    fn an_integer_field_writes_only_a_parsed_number_and_clears_only_an_optional_key(
        cx: &mut gpui::TestAppContext,
    ) {
        for (declared_default, typed, expected) in [
            (false, "48", Some(Variant::Int(48))),
            (false, "", None),
            (false, "   ", None),
            (false, "abc", Some(Variant::Int(SEEDED))),
            (false, "4.5", Some(Variant::Int(SEEDED))),
            (true, "48", Some(Variant::Int(48))),
            (true, "", Some(Variant::Int(SEEDED))),
            (true, "abc", Some(Variant::Int(SEEDED))),
        ] {
            let defaults = if declared_default {
                config(&[(NOTE_KEY, Variant::Int(0))])
            } else {
                FieldConfig::new()
            };
            let seeded = config(&[(NOTE_KEY, Variant::Int(SEEDED))]);
            let (_host, fields) = build(cx, &integer_spec(NOTE_KEY), &defaults, &seeded, typed);

            assert_eq!(
                collected(cx, &fields, &seeded).get(NOTE_KEY),
                expected.as_ref(),
                "declared default: {declared_default}, typed: {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn an_integer_field_saves_a_number_clamped_into_its_bounds(cx: &mut gpui::TestAppContext) {
        for (typed, expected) in [
            ("128", 127),
            ("1000000", 127),
            ("127", 127),
            ("126", 126),
            ("0", 0),
            ("1", 1),
            ("-1", 0),
            (" 200 ", 127),
        ] {
            let seeded = config(&[(NOTE_KEY, Variant::Int(SEEDED))]);
            let (_host, fields) = build(
                cx,
                &integer_spec(NOTE_KEY),
                &FieldConfig::new(),
                &seeded,
                typed,
            );

            assert_eq!(
                collected(cx, &fields, &seeded).get(NOTE_KEY),
                Some(&Variant::Int(expected)),
                "typed: {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn an_integer_spec_with_min_above_max_saves_a_number_without_panicking(
        cx: &mut gpui::TestAppContext,
    ) {
        let inverted = FormField::Integer {
            key: NOTE_KEY,
            label: "Note",
            min: 10,
            max: 5,
        };
        let (_host, fields) = build(cx, &inverted, &FieldConfig::new(), &FieldConfig::new(), "7");

        assert!(matches!(
            collected(cx, &fields, &FieldConfig::new()).get(NOTE_KEY),
            Some(Variant::Int(_))
        ));
    }

    #[test]
    fn clamped_integer_text_rewrites_only_an_out_of_range_number() {
        let bounds = 0..=127;
        for (typed, expected) in [
            ("128", Some("127")),
            ("-1", Some("0")),
            (" 300 ", Some("127")),
            ("0", None),
            ("127", None),
            ("64", None),
            ("", None),
            ("abc", None),
            ("4.5", None),
        ] {
            assert_eq!(
                clamped_integer_text(typed, &bounds).as_deref(),
                expected,
                "typed: {typed:?}"
            );
        }
    }

    #[gpui::test]
    fn leaving_or_submitting_an_out_of_range_number_snaps_the_text_to_the_bound(
        cx: &mut gpui::TestAppContext,
    ) {
        for (typed, shown) in [("500", "127"), ("-3", "0"), ("42", "42"), ("abc", "abc")] {
            for leave in [InputEvent::Submitted, InputEvent::Blurred] {
                let (_host, fields) = build(
                    cx,
                    &integer_spec(NOTE_KEY),
                    &FieldConfig::new(),
                    &FieldConfig::new(),
                    typed,
                );
                let ConfigField::Input { input, .. } = &fields[0] else {
                    panic!("an integer spec builds an input field");
                };
                input.update(cx, |_, cx| cx.emit(leave(typed.into())));
                cx.run_until_parked();

                assert_eq!(
                    input.read_with(cx, |field, _| field.content().to_owned()),
                    shown
                );
            }
        }
    }

    #[gpui::test]
    fn emptying_an_optional_number_leaves_every_other_key_standing(cx: &mut gpui::TestAppContext) {
        let seeded = config(&[
            (NOTE_KEY, Variant::Int(SEEDED)),
            ("device", Variant::String("Launchkey".into())),
        ]);
        let (_host, fields) = build(
            cx,
            &integer_spec(NOTE_KEY),
            &FieldConfig::new(),
            &seeded,
            "",
        );

        assert_eq!(
            collected(cx, &fields, &seeded),
            config(&[("device", Variant::String("Launchkey".into()))])
        );
    }

    #[gpui::test]
    fn an_optional_number_sharing_its_key_with_its_own_gate_erases_that_gate_when_emptied(
        cx: &mut gpui::TestAppContext,
    ) {
        let gated_on = config(&[(NOTE_KEY, Variant::Bool(true))]);
        let (_host, fields) = build(
            cx,
            &optional_integer_spec(NOTE_KEY),
            &FieldConfig::new(),
            &gated_on,
            "",
        );

        assert_eq!(collected(cx, &fields, &gated_on), FieldConfig::new());
    }

    fn dynamic_select(options_key: &'static str) -> FormField {
        FormField::DynamicSelect {
            key: "target",
            label: "Target",
            options_key,
        }
    }

    #[gpui::test]
    fn a_stored_raw_reward_id_survives_a_reload_and_resave_unchanged(
        cx: &mut gpui::TestAppContext,
    ) {
        let stored = config(&[("target", Variant::String("%reward.id%".into()))]);
        let (_host, fields) = build(
            cx,
            &dynamic_select("collections.twitch.rewards.manageable"),
            &FieldConfig::new(),
            &stored,
            "",
        );

        assert_eq!(collected(cx, &fields, &FieldConfig::new()), stored);
    }

    #[gpui::test]
    fn only_a_collection_select_becomes_a_choice_when_the_form_has_no_option_map(
        cx: &mut gpui::TestAppContext,
    ) {
        for (options_key, expect_choice) in [
            ("collections.twitch.rewards", true),
            ("obs.scene_names", true),
            ("vtube.model_ids", true),
            ("custom.names", false),
        ] {
            let (_host, fields) = build(
                cx,
                &dynamic_select(options_key),
                &FieldConfig::new(),
                &FieldConfig::new(),
                "",
            );

            assert_eq!(
                matches!(fields.as_slice(), [ConfigField::Choice { .. }]),
                expect_choice,
                "{options_key}"
            );
        }
    }

    #[test]
    fn the_donation_service_choice_offers_any_then_every_shipped_service() {
        use forge_registry::TriggerKindDescriptor as _;

        let specs = forge_runtime::triggers::DonationReceivedDescriptor.config_fields();
        let choices = CollectionChoices::for_specs(&specs);
        let mut fields = vec![choice("provider", "", None)];

        choices.refresh(&mut fields, &ChoiceOptions::new(), Some("Any"));

        let offered = match &fields[0] {
            ConfigField::Choice { options, .. } => options.clone(),
            _ => panic!("expected a choice field"),
        };
        assert_eq!(
            offered,
            [
                (String::new(), "Any".to_owned()),
                ("donatello".to_owned(), "Donatello".to_owned()),
                ("monobank".to_owned(), "monobank".to_owned()),
            ]
        );
    }

    const CODE_KEY: &str = "body";
    const CODE_GATE: &str = "use_body";
    const RHAI_BODY: &str = "let n = 1;\r\nif n > 0 {\n\tprint(\"привіт 🚀\");\n}\n";
    const JSON_BODY: &str = "{\n  \"items\": [1, 2.5, null],\n  \"name\": \"ü\"\n}";

    fn code_spec(language: CodeLanguage) -> FormField {
        FormField::Code {
            key: CODE_KEY,
            label: "Body",
            language,
        }
    }

    fn code_editor_of(fields: &[ConfigField]) -> Entity<CodeEditor> {
        match fields {
            [ConfigField::Code { editor, .. }] => editor.clone(),
            _ => panic!("expected a single code field"),
        }
    }

    #[gpui::test]
    fn a_code_field_writes_what_its_editor_holds_as_a_verbatim_string(
        cx: &mut gpui::TestAppContext,
    ) {
        for (language, stored, edited, expected) in [
            (CodeLanguage::Rhai, Some(RHAI_BODY), None, RHAI_BODY),
            (CodeLanguage::Json, Some(JSON_BODY), None, JSON_BODY),
            (CodeLanguage::Rhai, None, None, ""),
            (CodeLanguage::Json, Some(""), None, ""),
            (
                CodeLanguage::Rhai,
                Some(RHAI_BODY),
                Some("print(2);"),
                "print(2);",
            ),
            (CodeLanguage::Rhai, Some(RHAI_BODY), Some(""), ""),
        ] {
            let seeded: FieldConfig = stored
                .map(|body| (CODE_KEY.to_owned(), Variant::String(body.to_owned())))
                .into_iter()
                .collect();
            let (_host, fields) = build(cx, &code_spec(language), &FieldConfig::new(), &seeded, "");
            if let Some(text) = edited {
                let editor = code_editor_of(&fields);
                editor.update(cx, |editor, cx| editor.set_content(text, cx));
            }

            assert_eq!(
                collected(cx, &fields, &FieldConfig::new()).get(CODE_KEY),
                Some(&Variant::String(expected.to_owned())),
                "{language:?} stored {stored:?} edited {edited:?}"
            );
        }
    }

    #[gpui::test]
    fn a_gated_code_field_writes_only_while_its_toggle_is_on(cx: &mut gpui::TestAppContext) {
        for (toggle, expected) in [
            (true, Some(Variant::String(RHAI_BODY.to_owned()))),
            (false, None),
        ] {
            let spec = FormField::Optional {
                key: CODE_GATE,
                label: "Use body",
                inner: Box::new(code_spec(CodeLanguage::Rhai)),
            };
            let seeded = config(&[
                (CODE_GATE, Variant::Bool(toggle)),
                (CODE_KEY, Variant::String(RHAI_BODY.to_owned())),
            ]);
            let (_host, fields) = build(cx, &spec, &FieldConfig::new(), &seeded, "");

            assert_eq!(
                collected(cx, &fields, &FieldConfig::new()).get(CODE_KEY),
                expected.as_ref(),
                "toggle {toggle}"
            );
        }
    }

    struct CommitLog(Vec<String>);

    fn log_commit(log: &mut CommitLog, event: &InputEvent, _: &mut Context<CommitLog>) {
        let label = match event {
            InputEvent::Changed(_) => "changed",
            InputEvent::Submitted(_) => "submitted",
            InputEvent::Blurred(_) => "blurred",
            InputEvent::Cancelled => "cancelled",
        };
        log.0.push(label.to_owned());
    }

    #[gpui::test]
    fn a_code_field_hands_only_a_blur_commit_to_the_form(cx: &mut gpui::TestAppContext) {
        let host = cx.update(|cx| cx.new(|_| CommitLog(Vec::new())));
        let fields = host.update(cx, |_, cx| {
            let palette = forge_components::ThemeId::ForgeDefault.palette();
            let ctx = FoldContext {
                config: &FieldConfig::new(),
                defaults: &FieldConfig::new(),
                palette: &palette,
                choices: ChoiceSupport::Text,
                on_committed: log_commit as ConfigCommitHandler<CommitLog>,
            };
            let mut fields = Vec::new();
            fold_config_field(&code_spec(CodeLanguage::Rhai), None, &ctx, &mut fields, cx);
            fields
        });
        let editor = code_editor_of(&fields);

        editor.update(cx, |_, cx| {
            cx.emit(InputEvent::Changed("a".into()));
            cx.emit(InputEvent::Submitted("a".into()));
            cx.emit(InputEvent::Cancelled);
            cx.emit(InputEvent::Blurred("a".into()));
        });
        cx.run_until_parked();

        assert_eq!(host.read_with(cx, |log, _| log.0.clone()), ["blurred"]);
    }
}
