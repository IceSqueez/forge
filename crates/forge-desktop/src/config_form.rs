use std::collections::{BTreeMap, HashMap};

use forge_components::{
    BORDER_THIN, Density, ForgePalette, Icon, InputEvent, Radius, Spacing, TextInput,
    accent_swatch, body_family, icon, mono_family, radius, slider, spacing, toggle, tr,
};
use forge_registry::FormField;
use forge_types::Variant;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Point, SharedString, Subscription,
    Window, div, prelude::*, px,
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

const CHOICE_PAD_V: Pixels = px(6.0);
const CHOICE_PAD_H: Pixels = px(9.0);
const CHOICE_GLYPH: Pixels = px(12.0);

type FieldConfig = BTreeMap<String, Variant>;

pub(crate) enum ConfigField {
    Input {
        key: String,
        integer: bool,
        gate: Option<String>,
        input: Entity<TextInput>,
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
            | Self::Bool { key, .. }
            | Self::Slide { key, .. }
            | Self::Swatch { key, .. }
            | Self::Choice { key, .. }
            | Self::Hint { key } => key,
        }
    }
}

pub(crate) type ConfigCommitHandler<V> =
    fn(&mut V, Entity<TextInput>, &InputEvent, &mut Context<V>);

type ChoiceOpener<V> = fn(&mut V, String, Point<Pixels>, &mut Window, &mut Context<V>);

/// Whether the calling view hosts a value picker. Without one, the select-shaped fields fall back
/// to free text rather than rendering a control whose click would go nowhere.
pub(crate) enum ChoiceSupport<'a> {
    Text,
    Picker(&'a HashMap<String, Vec<(String, String)>>),
}

pub(crate) struct ConfigFieldHandlers<V: 'static> {
    pub(crate) toggle: fn(&mut V, String, &mut Context<V>),
    pub(crate) slide: fn(&mut V, String, i64, &mut Context<V>),
    pub(crate) pick: fn(&mut V, String, String, &mut Context<V>),
    pub(crate) open_choice: Option<ChoiceOpener<V>>,
}

pub(crate) fn sparse_overrides(default: &FieldConfig, buffer: &FieldConfig) -> FieldConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Everything a fold needs beyond the field itself, so the walk stays one call per spec.
pub(crate) struct FoldContext<'a, V: 'static> {
    pub(crate) config: &'a FieldConfig,
    pub(crate) palette: &'a ForgePalette,
    pub(crate) choices: ChoiceSupport<'a>,
    pub(crate) on_committed: ConfigCommitHandler<V>,
}

pub(crate) fn fold_config_field<V: 'static>(
    spec: &FormField,
    gate: Option<String>,
    ctx: &FoldContext<'_, V>,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<V>,
) {
    let FoldContext {
        config,
        palette,
        choices,
        on_committed,
    } = ctx;
    let on_committed = *on_committed;
    match spec {
        FormField::Text {
            key, placeholder, ..
        } => out.push(build_config_input(
            key,
            placeholder,
            false,
            gate,
            config,
            palette,
            on_committed,
            cx,
        )),
        FormField::TextArea { key, .. } | FormField::Code { key, .. } => out.push(
            build_config_input(key, "", false, gate, config, palette, on_committed, cx),
        ),
        FormField::Integer { key, .. } => {
            out.push(build_config_input(
                key,
                "0",
                true,
                gate,
                config,
                palette,
                on_committed,
                cx,
            ));
        }
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
            value: read_int(config, key).unwrap_or(*min).clamp(*min, *max),
        }),
        FormField::Swatch { key, options, .. } => out.push(ConfigField::Swatch {
            key: (*key).to_owned(),
            gate,
            options: options
                .iter()
                .map(|name| {
                    let tint = accent_swatch(name, palette).unwrap_or(palette.text_faint);
                    ((*name).to_owned(), tint)
                })
                .collect(),
            selected: read_text(config, key),
        }),
        FormField::Select { key, options, .. } => match choices {
            ChoiceSupport::Text => out.push(build_config_input(
                key,
                "",
                false,
                gate,
                config,
                palette,
                on_committed,
                cx,
            )),
            ChoiceSupport::Picker(_) => out.push(ConfigField::Choice {
                key: (*key).to_owned(),
                gate,
                options: options
                    .iter()
                    .map(|opt| ((*opt).to_owned(), (*opt).to_owned()))
                    .collect(),
                selected: read_text(config, key),
                dependency: None,
            }),
        },
        FormField::DynamicSelect {
            key, options_key, ..
        } => match choices {
            ChoiceSupport::Text => out.push(build_config_input(
                key,
                "",
                false,
                gate,
                config,
                palette,
                on_committed,
                cx,
            )),
            ChoiceSupport::Picker(map) => out.push(ConfigField::Choice {
                key: (*key).to_owned(),
                gate,
                options: map.get(*options_key).cloned().unwrap_or_default(),
                selected: read_text(config, key),
                dependency: None,
            }),
        },
        FormField::DependentSelect {
            key,
            options_prefix,
            depends_on,
            ..
        } => match choices {
            ChoiceSupport::Text => out.push(build_config_input(
                key,
                "",
                false,
                gate,
                config,
                palette,
                on_committed,
                cx,
            )),
            ChoiceSupport::Picker(map) => out.push(ConfigField::Choice {
                key: (*key).to_owned(),
                gate,
                options: dependent_options(map, options_prefix, &read_text(config, depends_on)),
                selected: read_text(config, key),
                dependency: Some(ChoiceDependency {
                    options_prefix: (*options_prefix).to_owned(),
                    depends_on: (*depends_on).to_owned(),
                }),
            }),
        },
        FormField::FilePicker { key, .. } | FormField::DateTime { key, .. } => out.push(
            build_config_input(key, "", false, gate, config, palette, on_committed, cx),
        ),
        FormField::Toggle { key, .. } => {
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate,
                value: matches!(config.get(*key), Some(Variant::Bool(true))),
            });
        }
        FormField::SubChain { key, .. } | FormField::CaseList { key, .. } => {
            out.push(ConfigField::Hint {
                key: (*key).to_owned(),
            });
        }
        FormField::Optional { key, inner, .. } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate: gate.clone(),
                value,
            });
            fold_config_field(inner, Some((*key).to_owned()), ctx, out, cx);
        }
    }
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

#[allow(clippy::too_many_arguments)]
fn build_config_input<V: 'static>(
    key: &str,
    placeholder: &'static str,
    integer: bool,
    gate: Option<String>,
    config: &FieldConfig,
    palette: &ForgePalette,
    on_committed: ConfigCommitHandler<V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let seed = read_text(config, key);
    let palette = *palette;
    let input = cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx).with_palette(palette);
        if !seed.is_empty() {
            input.set_content(seed, cx);
        }
        input
    });
    let sub = cx.subscribe(&input, on_committed);
    ConfigField::Input {
        key: key.to_owned(),
        integer,
        gate,
        input,
        _sub: sub,
    }
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
            }
            | ConfigField::Choice {
                key,
                gate,
                selected,
                ..
            } => {
                if gate_on(gate) && !selected.is_empty() {
                    buffer.insert(key.clone(), Variant::String(selected.clone()));
                }
            }
            ConfigField::Input {
                key,
                integer,
                gate,
                input,
                ..
            } => {
                if !gate_on(gate) {
                    continue;
                }
                let text = input.read(cx).content().to_owned();
                if *integer {
                    if let Ok(n) = text.trim().parse::<i64>() {
                        buffer.insert(key.clone(), Variant::Int(n));
                    }
                } else {
                    buffer.insert(key.clone(), Variant::String(text));
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
                .w(SLIDER_READOUT_W)
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

    if let Some(open_choice) = handlers.open_choice {
        let open_key = key.to_owned();
        let view = view.clone();
        let hover_border = palette.brand;
        trigger = trigger
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(
                move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                    let position = event.position();
                    view.update(cx, |this, cx| {
                        open_choice(this, open_key.clone(), position, window, cx)
                    });
                },
            );
    }

    trigger.into_any_element()
}

pub(crate) fn render_config_row<V: 'static>(
    field: &ConfigField,
    last: bool,
    palette: &ForgePalette,
    id_prefix: &str,
    view: &Entity<V>,
    handlers: &ConfigFieldHandlers<V>,
) -> AnyElement {
    let label = div()
        .w(FILL_KEY_W)
        .flex_none()
        .overflow_hidden()
        .font_family(mono_family())
        .text_size(FILL_KEY_FS)
        .text_color(palette.text_muted)
        .child(field.key().to_owned());

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
}
