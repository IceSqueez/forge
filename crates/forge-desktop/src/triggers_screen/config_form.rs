use forge_components::{ForgePalette, InputEvent, TextInput};
use forge_registry::FormField;
use forge_types::{TriggerConfig, Variant};
use gpui::{App, Context, Entity, Subscription, prelude::*};
use std::collections::HashMap;

use super::TriggersRegistryView;

pub(super) enum ConfigField {
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

pub(super) type ConfigCommitHandler = fn(
    &mut TriggersRegistryView,
    Entity<TextInput>,
    &InputEvent,
    &mut Context<TriggersRegistryView>,
);

pub(super) fn variant_display(v: &Variant) -> String {
    match v {
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Bool(b) => b.to_string(),
        Variant::String(s) => s.clone(),
        Variant::Datetime(dt) => dt.to_string(),
        Variant::Array(_) | Variant::Object(_) => String::new(),
    }
}

pub(super) fn sparse_overrides(default: &TriggerConfig, buffer: &TriggerConfig) -> TriggerConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub(super) fn fold_config_field(
    spec: &FormField,
    gate: Option<String>,
    config: &TriggerConfig,
    palette: &ForgePalette,
    on_committed: ConfigCommitHandler,
    out: &mut Vec<ConfigField>,
    cx: &mut Context<TriggersRegistryView>,
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
        FormField::TextArea { key, .. } => out.push(build_config_input(
            key,
            "",
            false,
            gate,
            config,
            palette,
            on_committed,
            cx,
        )),
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
fn build_config_input(
    key: &str,
    placeholder: &'static str,
    integer: bool,
    gate: Option<String>,
    config: &TriggerConfig,
    palette: &ForgePalette,
    on_committed: ConfigCommitHandler,
    cx: &mut Context<TriggersRegistryView>,
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

pub(super) fn overlay_field_values(fields: &[ConfigField], buffer: &mut TriggerConfig, cx: &App) {
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
