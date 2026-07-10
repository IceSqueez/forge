use std::collections::BTreeMap;

use forge_registry::FormField;
use forge_types::Variant;
use iced::{
    Background, Border, Element, Length,
    widget::{column, container, text},
};

use forge_widgets::{
    ForgePalette, Radius, Spacing, ToggleProps, radius, spf, text_input_field, toggle,
    tokens::{BORDER_THIN, FONT_XS, FontRole, font},
};

#[derive(Debug, Clone)]
pub enum FieldEditMsg {
    Set(String, Variant),
    IntInput(String, String),
    Clear(String),
}

/// Runtime-supplied option lists keyed by `FormField::DynamicSelect.options_key`.
/// Each entry maps a stored config value to its display label.
pub type DynamicOptions<'a> = BTreeMap<&'a str, Vec<(String, String)>>;

pub struct FieldBuffers<'a> {
    pub text: &'a BTreeMap<String, String>,
    pub overrides: &'a BTreeMap<String, Variant>,
}

pub fn variant_to_display_str(v: &Variant) -> String {
    match v {
        Variant::Int(n) => n.to_string(),
        Variant::Float(f) => f.to_string(),
        Variant::Bool(b) => b.to_string(),
        Variant::String(s) => s.clone(),
        Variant::Datetime(dt) => dt.to_string(),
        Variant::Array(_) | Variant::Object(_) => String::new(),
    }
}

/// Keeps only the buffer entries that diverge from `default_config`, so a saved
/// config stores a sparse diff rather than a full snapshot. Consumers restore
/// the whole config via `forge_registry::effective_config`; storing the full
/// buffer would freeze today's defaults into the row and mask later default
/// changes for that field.
pub fn sparse_overrides(
    default_config: &BTreeMap<String, Variant>,
    buffer: &BTreeMap<String, Variant>,
) -> BTreeMap<String, Variant> {
    buffer
        .iter()
        .filter(|(k, v)| default_config.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn dynamic_pick_list<'a, Message: Clone + 'a>(
    options: &[(String, String)],
    current_value: Option<&str>,
    palette: &ForgePalette,
    on_select: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    let labels: Vec<String> = options.iter().map(|(_, label)| label.clone()).collect();
    let selected: Option<String> = current_value.and_then(|v| {
        options
            .iter()
            .find(|(value, _)| value == v)
            .map(|(_, label)| label.clone())
    });
    let value_by_label: BTreeMap<String, String> = options
        .iter()
        .map(|(value, label)| (label.clone(), value.clone()))
        .collect();
    let p = *palette;
    iced::widget::pick_list(labels, selected, move |label: String| {
        let value = value_by_label.get(&label).cloned().unwrap_or(label);
        on_select(value)
    })
    .padding(forge_widgets::input_padding())
    .width(Length::Fill)
    .style(move |_theme, status| {
        use iced::widget::pick_list;
        let border_color = match status {
            pick_list::Status::Opened { .. } => p.border_active,
            _ => p.border_input,
        };
        pick_list::Style {
            text_color: p.text_primary,
            placeholder_color: p.text_muted,
            handle_color: p.text_muted,
            background: Background::Color(p.shell),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
        }
    })
    .into()
}

pub fn render_field<'a, Message, F>(
    field: &FormField,
    buffers: &FieldBuffers<'a>,
    options: &DynamicOptions<'a>,
    palette: &'a ForgePalette,
    on_edit: F,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    F: Fn(FieldEditMsg) -> Message + Copy + 'a,
{
    let p = *palette;
    let field_row_padding = [spf(Spacing::Xxs), 0.0];

    match field {
        FormField::Text {
            key,
            label,
            placeholder,
        } => {
            let display = buffers.text.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            let cb = move |v: String| on_edit(FieldEditMsg::Set(k.clone(), Variant::String(v)));
            column![
                field_label(label, p),
                text_input_field(*placeholder, display, cb, palette),
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::TextArea { key, label } => {
            let display = buffers.text.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            let cb = move |v: String| on_edit(FieldEditMsg::Set(k.clone(), Variant::String(v)));
            column![
                field_label(label, p),
                text_input_field("", display, cb, palette)
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::Integer { key, label, .. } => {
            let display = buffers.text.get(*key).map(|s| s.as_str()).unwrap_or("");
            let k = key.to_string();
            let cb = move |v: String| on_edit(FieldEditMsg::IntInput(k.clone(), v));
            column![
                field_label(label, p),
                text_input_field("0", display, cb, palette)
            ]
            .spacing(spf(Spacing::Xxs))
            .padding(field_row_padding)
            .into()
        }
        FormField::Toggle { key, label } => {
            let checked = matches!(buffers.overrides.get(*key), Some(Variant::Bool(true)));
            let k = key.to_string();
            container(toggle(
                palette,
                ToggleProps {
                    label: label.to_string(),
                    description: String::new(),
                    value: checked,
                    on_toggle: on_edit(FieldEditMsg::Set(k, Variant::Bool(!checked))),
                    accent: None,
                },
            ))
            .padding(field_row_padding)
            .into()
        }
        FormField::Select {
            key,
            label,
            options: choices,
        } => {
            let opts: Vec<(String, String)> = choices
                .iter()
                .map(|s| (s.to_string(), s.to_string()))
                .collect();
            let current = buffers.overrides.get(*key).and_then(|v| match v {
                Variant::String(s) => Some(s.clone()),
                _ => None,
            });
            let k = key.to_string();
            let picker = dynamic_pick_list(&opts, current.as_deref(), palette, move |value| {
                on_edit(FieldEditMsg::Set(k.clone(), Variant::String(value)))
            });
            column![field_label(label, p), picker]
                .spacing(spf(Spacing::Xxs))
                .padding(field_row_padding)
                .into()
        }
        FormField::DynamicSelect {
            key,
            label,
            options_key,
        } => {
            let current = buffers.overrides.get(*key).and_then(|v| match v {
                Variant::String(s) => Some(s.clone()),
                _ => None,
            });
            match options.get(options_key) {
                Some(opts) if !opts.is_empty() => {
                    let k = key.to_string();
                    let picker =
                        dynamic_pick_list(opts, current.as_deref(), palette, move |value| {
                            on_edit(FieldEditMsg::Set(k.clone(), Variant::String(value)))
                        });
                    column![field_label(label, p), picker]
                        .spacing(spf(Spacing::Xxs))
                        .padding(field_row_padding)
                        .into()
                }
                _ => {
                    let display = buffers.text.get(*key).map(|s| s.as_str()).unwrap_or("");
                    let k = key.to_string();
                    let cb =
                        move |v: String| on_edit(FieldEditMsg::Set(k.clone(), Variant::String(v)));
                    let placeholder = format!("Enter value  ({options_key})");
                    column![
                        field_label(label, p),
                        text_input_field(placeholder, display, cb, palette)
                    ]
                    .spacing(spf(Spacing::Xxs))
                    .padding(field_row_padding)
                    .into()
                }
            }
        }
        FormField::SubChain { label, .. } | FormField::CaseList { label, .. } => {
            // Nested sub-chains are authored in the recursive step-list surface
            // (drill-in from the action detail pane), never as a text input here.
            let hint = text(forge_widgets::tr!("action_editor_branch_modal_hint"))
                .size(FONT_XS)
                .color(p.text_faint)
                .font(font(FontRole::Body));
            column![field_label(label, p), hint]
                .spacing(spf(Spacing::Xxs))
                .padding(field_row_padding)
                .into()
        }
        FormField::Optional { key, label, inner } => {
            let is_enabled = matches!(buffers.overrides.get(*key), Some(Variant::Bool(true)));
            let k = key.to_string();
            let toggle_el = toggle(
                palette,
                ToggleProps {
                    label: label.to_string(),
                    description: String::new(),
                    value: is_enabled,
                    on_toggle: on_edit(FieldEditMsg::Set(k, Variant::Bool(!is_enabled))),
                    accent: None,
                },
            );
            if is_enabled {
                column![
                    toggle_el,
                    render_field(inner, buffers, options, palette, on_edit)
                ]
                .spacing(spf(Spacing::Xs))
                .padding(field_row_padding)
                .into()
            } else {
                container(toggle_el).padding(field_row_padding).into()
            }
        }
    }
}

fn field_label<'a, Message: 'a>(label: &str, p: ForgePalette) -> Element<'a, Message> {
    text(label.to_owned())
        .size(FONT_XS)
        .color(p.text_secondary)
        .font(font(FontRole::Body))
        .into()
}

/// Applies one field-edit to the paired text/override buffers; keeps the
/// displayed text and the typed `Variant` in sync (integer parse is lenient —
/// invalid input stays visible without dropping the prior numeric value).
pub fn apply_field_edit(
    text_buffer: &mut BTreeMap<String, String>,
    overrides_buffer: &mut BTreeMap<String, Variant>,
    edit: FieldEditMsg,
) {
    match edit {
        FieldEditMsg::Set(key, variant) => {
            text_buffer.insert(key.clone(), variant_to_display_str(&variant));
            overrides_buffer.insert(key, variant);
        }
        FieldEditMsg::IntInput(key, raw) => {
            text_buffer.insert(key.clone(), raw.clone());
            if let Ok(n) = raw.parse::<i64>() {
                overrides_buffer.insert(key, Variant::Int(n));
            }
        }
        FieldEditMsg::Clear(key) => {
            overrides_buffer.remove(&key);
            text_buffer.remove(&key);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn buffers() -> (BTreeMap<String, String>, BTreeMap<String, Variant>) {
        (BTreeMap::new(), BTreeMap::new())
    }

    #[test]
    fn set_writes_both_the_display_text_and_the_typed_override() {
        let (mut text, mut overrides) = buffers();
        apply_field_edit(
            &mut text,
            &mut overrides,
            FieldEditMsg::Set("msg".into(), Variant::String("hi".into())),
        );
        assert_eq!(text.get("msg").map(String::as_str), Some("hi"));
        assert_eq!(overrides.get("msg"), Some(&Variant::String("hi".into())));
    }

    #[test]
    fn int_input_with_valid_digits_sets_int_override_and_keeps_raw_text() {
        let (mut text, mut overrides) = buffers();
        apply_field_edit(
            &mut text,
            &mut overrides,
            FieldEditMsg::IntInput("n".into(), "42".into()),
        );
        assert_eq!(text.get("n").map(String::as_str), Some("42"));
        assert_eq!(overrides.get("n"), Some(&Variant::Int(42)));
    }

    #[test]
    fn int_input_with_non_numeric_keeps_raw_text_but_preserves_prior_numeric_override() {
        let (mut text, mut overrides) = buffers();
        apply_field_edit(
            &mut text,
            &mut overrides,
            FieldEditMsg::IntInput("n".into(), "7".into()),
        );
        // User backspaces into an invalid intermediate state ("7a").
        apply_field_edit(
            &mut text,
            &mut overrides,
            FieldEditMsg::IntInput("n".into(), "7a".into()),
        );
        // Lenient-parse contract: raw text stays visible...
        assert_eq!(text.get("n").map(String::as_str), Some("7a"));
        // ...but the last successfully-parsed numeric value is NOT clobbered.
        assert_eq!(overrides.get("n"), Some(&Variant::Int(7)));
    }

    #[test]
    fn int_input_with_non_numeric_and_no_prior_value_leaves_override_unset() {
        let (mut text, mut overrides) = buffers();
        apply_field_edit(
            &mut text,
            &mut overrides,
            FieldEditMsg::IntInput("n".into(), "abc".into()),
        );
        assert_eq!(text.get("n").map(String::as_str), Some("abc"));
        assert!(!overrides.contains_key("n"));
    }

    #[test]
    fn clear_removes_the_key_from_both_buffers() {
        let (mut text, mut overrides) = buffers();
        apply_field_edit(
            &mut text,
            &mut overrides,
            FieldEditMsg::Set("k".into(), Variant::Bool(true)),
        );
        apply_field_edit(&mut text, &mut overrides, FieldEditMsg::Clear("k".into()));
        assert!(!text.contains_key("k"));
        assert!(!overrides.contains_key("k"));
    }

    #[test]
    fn variant_to_display_str_renders_scalar_variants() {
        assert_eq!(variant_to_display_str(&Variant::Int(7)), "7");
        assert_eq!(variant_to_display_str(&Variant::Bool(true)), "true");
        assert_eq!(variant_to_display_str(&Variant::String("x".into())), "x");
        // Float renders through its own Display, not a fixed precision.
        assert_eq!(variant_to_display_str(&Variant::Float(1.5)), "1.5");
    }

    #[test]
    fn variant_to_display_str_collapses_collections_to_empty_string() {
        // Contract: Array/Object are not text-editable in the field form, so
        // they intentionally render blank rather than a Debug dump.
        assert_eq!(
            variant_to_display_str(&Variant::Array(vec![Variant::Int(1)])),
            ""
        );
        let mut obj = BTreeMap::new();
        obj.insert("a".to_owned(), Variant::Int(1));
        assert_eq!(variant_to_display_str(&Variant::Object(obj)), "");
    }

    #[test]
    fn sparse_overrides_keeps_only_entries_that_diverge_from_default() {
        // Two-key default the saved instance config is diffed against.
        let default = || {
            BTreeMap::from([
                ("mode".to_owned(), Variant::String("cheer".to_owned())),
                ("threshold".to_owned(), Variant::Int(100)),
            ])
        };

        // (label, buffer, expected sparse result). Each row pins the SPARSE-1
        // invariant end-to-end, not a restatement of the filter predicate.
        type Config = BTreeMap<String, Variant>;
        let cases: Vec<(&str, Config, Config)> = vec![
            // Revert-to-default — the whole point of the fix: a buffer entry that
            // re-states today's default MUST be pruned, so a later default change
            // is not masked by a frozen snapshot of the old value.
            (
                "entry equal to default is pruned",
                BTreeMap::from([("threshold".to_owned(), Variant::Int(100))]),
                BTreeMap::new(),
            ),
            // Genuine override survives verbatim.
            (
                "entry differing from default is kept with the buffer value",
                BTreeMap::from([("threshold".to_owned(), Variant::Int(250))]),
                BTreeMap::from([("threshold".to_owned(), Variant::Int(250))]),
            ),
            // Boundary: key absent from default_config (default.get(k) == None,
            // so None != Some(v)) is kept rather than dropped.
            (
                "entry whose key is absent from default is kept",
                BTreeMap::from([("extra".to_owned(), Variant::Bool(true))]),
                BTreeMap::from([("extra".to_owned(), Variant::Bool(true))]),
            ),
            // The exact bug the fix closes: a full snapshot of the defaults must
            // collapse to empty — no full snapshots are ever stored again.
            (
                "buffer identical to full default collapses to empty",
                default(),
                BTreeMap::new(),
            ),
            (
                "empty buffer yields empty result",
                BTreeMap::new(),
                BTreeMap::new(),
            ),
        ];

        for (label, buffer, expected) in cases {
            assert_eq!(sparse_overrides(&default(), &buffer), expected, "{label}");
        }
    }
}
