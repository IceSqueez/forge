use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, ForgePalette, InputEvent,
    Spacing, TextInput, spacing, toggle, tr,
};
use forge_registry::FormField;
use forge_types::{TriggerConfig, Variant};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, SharedString, Subscription, Window, div,
    prelude::*, px,
};
use std::collections::HashMap;

const FILL_KEY_W: Pixels = px(110.0);
const FILL_KEY_FS: Pixels = px(11.0);
pub(crate) const FILL_VAL_FS: Pixels = px(11.5);
const FILL_ROW_PAD_V: Pixels = px(8.0);
const FILL_ROW_PAD_H: Pixels = px(12.0);

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
    Hint {
        key: String,
    },
}

pub(crate) type ConfigCommitHandler<V> =
    fn(&mut V, Entity<TextInput>, &InputEvent, &mut Context<V>);

fn variant_display(v: &Variant) -> String {
    match v {
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Bool(b) => b.to_string(),
        Variant::String(s) => s.clone(),
        Variant::Datetime(dt) => dt.to_string(),
        Variant::Array(_) | Variant::Object(_) => String::new(),
    }
}

pub(crate) fn sparse_overrides(default: &TriggerConfig, buffer: &TriggerConfig) -> TriggerConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub(crate) fn fold_config_field<V: 'static>(
    spec: &FormField,
    gate: Option<String>,
    config: &TriggerConfig,
    palette: &ForgePalette,
    on_committed: ConfigCommitHandler<V>,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<V>,
) {
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
        FormField::Integer { key, .. } => out.push(build_config_input(
            key,
            "0",
            true,
            gate,
            config,
            palette,
            on_committed,
            cx,
        )),
        // Select / DynamicSelect degrade to free-text: the kit ships no value-picker yet.
        FormField::Select { key, .. } | FormField::DynamicSelect { key, .. } => out.push(
            build_config_input(key, "", false, gate, config, palette, on_committed, cx),
        ),
        FormField::FilePicker { key, .. } => out.push(build_config_input(
            key,
            "",
            false,
            gate,
            config,
            palette,
            on_committed,
            cx,
        )),
        FormField::Toggle { key, .. } => {
            let value = matches!(config.get(*key), Some(Variant::Bool(true)));
            out.push(ConfigField::Bool {
                key: (*key).to_owned(),
                gate,
                value,
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
            fold_config_field(
                inner,
                Some((*key).to_owned()),
                config,
                palette,
                on_committed,
                out,
                cx,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_config_input<V: 'static>(
    key: &str,
    placeholder: &'static str,
    integer: bool,
    gate: Option<String>,
    config: &TriggerConfig,
    palette: &ForgePalette,
    on_committed: ConfigCommitHandler<V>,
    cx: &mut Context<V>,
) -> ConfigField {
    let seed = config.get(key).map(variant_display).unwrap_or_default();
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

pub(crate) fn overlay_field_values(fields: &[ConfigField], buffer: &mut TriggerConfig, cx: &App) {
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

pub(crate) fn render_config_row<V: 'static>(
    field: &ConfigField,
    last: bool,
    palette: &ForgePalette,
    toggle_id_prefix: &str,
    view: &Entity<V>,
    on_toggle: fn(&mut V, String, &mut Context<V>),
) -> AnyElement {
    let key = match field {
        ConfigField::Input { key, .. }
        | ConfigField::Bool { key, .. }
        | ConfigField::Hint { key } => key.clone(),
    };

    let label = div()
        .w(FILL_KEY_W)
        .flex_none()
        .overflow_hidden()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FILL_KEY_FS)
        .text_color(palette.text_muted)
        .child(key.clone());

    let value: AnyElement = match field {
        ConfigField::Input { input, .. } => div().child(input.clone()).into_any_element(),
        ConfigField::Bool { key, value, .. } => {
            let toggle_key = key.clone();
            let view = view.clone();
            toggle(*value, palette)
                .on_click(
                    SharedString::from(format!("{toggle_id_prefix}-{key}")),
                    move |_: &ClickEvent, _window: &mut Window, cx: &mut App| {
                        view.update(cx, |this, cx| on_toggle(this, toggle_key.clone(), cx));
                    },
                )
                .into_any_element()
        }
        ConfigField::Hint { .. } => div()
            .italic()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FILL_VAL_FS)
            .text_color(palette.text_faint)
            .child(tr!("triggers_sheet_config_authored"))
            .into_any_element(),
    };

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
        .child(div().flex_1().min_w(px(0.0)).child(value))
        .into_any_element()
}
