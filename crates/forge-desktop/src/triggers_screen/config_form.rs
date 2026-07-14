//! Shared configuration-editor folding for the triggers screen: the `FormField` →
//! `ConfigField` fold (the detail side-sheet and the create-form both build their
//! editor rows through it) and the reverse overlay of the edited field values back
//! onto a config buffer. Each screen renders its `ConfigField` rows locally.

use forge_components::{ForgePalette, InputEvent, TextInput};
use forge_registry::FormField;
use forge_types::{TriggerConfig, Variant};
use gpui::{App, Context, Entity, Subscription, prelude::*};
use std::collections::HashMap;

use super::TriggersRegistryView;

/// One row in a config editor, folded from the kind's `config_fields` over the
/// effective (default-merged) config. `Hint` marks a key authored elsewhere (a nested
/// sub-chain), rendered inert.
pub(super) enum ConfigField {
    Input {
        key: String,
        /// Committed as `Variant::Int` (lenient parse — a non-numeric value keeps the
        /// field's prior value) rather than `Variant::String`.
        integer: bool,
        /// Set on the inner member of an `Optional` group; committed only while the
        /// gate toggle (a sibling `Bool` on this key) is on.
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

/// The submit handler a screen wires onto every config input it folds, so pressing
/// Enter routes back to that screen's commit path.
pub(super) type ConfigCommitHandler = fn(
    &mut TriggersRegistryView,
    Entity<TextInput>,
    &InputEvent,
    &mut Context<TriggersRegistryView>,
);

/// Renders a `Variant` as the single-line string the field editor seeds and commits.
/// Composite values carry no inline text form.
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

/// Keeps only the buffer entries diverging from `default`, so a saved config stores a
/// sparse diff the runtime re-merges over the current defaults rather than freezing
/// today's defaults into the row.
pub(super) fn sparse_overrides(default: &TriggerConfig, buffer: &TriggerConfig) -> TriggerConfig {
    buffer
        .iter()
        .filter(|(k, v)| default.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Folds one `FormField` (recursing through `Optional`) into the flat config-editor
/// field list, seeding each input from `config` and wiring `on_committed` to every
/// input. Select / DynamicSelect degrade to a free-text input — the kit ships no
/// value-picker primitive yet.
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

/// Overlays the editor's live field values onto `buffer`, gated the same way the
/// runtime re-merges an `Optional` group: an inner member commits only while its gate
/// toggle is on. Integer inputs keep the buffer's prior value on a non-numeric parse.
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
