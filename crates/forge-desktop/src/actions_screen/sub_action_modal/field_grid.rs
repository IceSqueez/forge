use super::field::{SubFormField, field_keys, is_var_key};
use super::field_controls::{bool_row, field_wrap, hint_block, multiline_field};
use super::rejection::with_rejection_note;
use super::*;
use forge_components::{FONT_SM, Spacing, body_family, spacing};
use forge_registry::CodeLanguage;
use gpui::Div;

impl EditSubActionForm {
    pub(super) fn render_field_grid(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> (Div, bool) {
        let bool_vals: HashMap<&str, bool> = self
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.as_str(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: &Option<String>| {
            gate.as_ref()
                .map(|g| bool_vals.get(g.as_str()).copied().unwrap_or(false))
                .unwrap_or(true)
        };

        let rejected_key = self
            .rejection
            .as_ref()
            .and_then(|rejection| rejection.field_key.as_deref());
        let mut rejection_shown_inline = false;
        let mut grid_items: Vec<(bool, AnyElement)> = Vec::new();
        for field in &self.fields {
            let pushed_before = grid_items.len();
            match field {
                SubFormField::Input {
                    key,
                    label,
                    browse,
                    datetime,
                    gate,
                    input,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let control = self.input_control(key, *browse, *datetime, input, palette, cx);
                    grid_items.push((
                        sub_field_is_half(field),
                        field_wrap(label, control, palette),
                    ));
                }
                SubFormField::Area {
                    label, gate, area, ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    grid_items.push((
                        false,
                        multiline_field(label, None, area.clone().into_any_element(), palette),
                    ));
                }
                SubFormField::Code {
                    label,
                    gate,
                    language,
                    editor,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let tag = match language {
                        CodeLanguage::Rhai => "rhai",
                        CodeLanguage::Json => "json",
                    };
                    grid_items.push((
                        false,
                        multiline_field(
                            label,
                            Some(tag),
                            editor.clone().into_any_element(),
                            palette,
                        ),
                    ));
                }
                SubFormField::Bool {
                    key,
                    label,
                    gate,
                    value,
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    grid_items.push((false, bool_row(key, label, *value, palette, cx)));
                }
                SubFormField::Select {
                    key,
                    label,
                    options,
                    gate,
                    selected,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let open_picker = self.open_picker_for(key);
                    grid_items.push((
                        true,
                        self.render_select_field(
                            key,
                            label,
                            options,
                            selected,
                            open_picker,
                            palette,
                            cx,
                        ),
                    ));
                }
                SubFormField::UnitAmount {
                    label,
                    unit_picker_key,
                    scale,
                    unit,
                    gate,
                    input,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let control =
                        self.unit_amount_control(unit_picker_key, scale, unit, input, palette, cx);
                    grid_items.push((false, field_wrap(label, control, palette)));
                }
                SubFormField::Hint { label } => {
                    grid_items.push((false, hint_block(label, palette)));
                }
            }
            let names_rejected_key =
                rejected_key.is_some_and(|rejected| field_keys(field).contains(&rejected));
            if names_rejected_key
                && grid_items.len() > pushed_before
                && let Some(message) = self.rejection.as_ref().map(|r| r.message.clone())
                && let Some((half, element)) = grid_items.pop()
            {
                rejection_shown_inline = true;
                grid_items.push((half, with_rejection_note(element, message, palette)));
            }
        }

        (
            field_grid_layout(grid_items, palette),
            rejection_shown_inline,
        )
    }
}

fn field_grid_layout(grid_items: Vec<(bool, AnyElement)>, palette: &ForgePalette) -> Div {
    let mut grid = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Sm, Density::Cozy));
    if grid_items.is_empty() {
        grid = grid.child(
            div()
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("actions_sub_no_config")),
        );
    } else {
        let mut it = grid_items.into_iter().peekable();
        while let Some((half, element)) = it.next() {
            if half {
                let second = if it.peek().map(|(h, _)| *h).unwrap_or(false) {
                    it.next().map(|(_, e)| e)
                } else {
                    None
                };
                let right = match second {
                    Some(e) => div().flex_1().child(e),
                    None => div().flex_1(),
                };
                grid = grid.child(
                    div()
                        .flex()
                        .gap(GRID_COL_GAP)
                        .child(div().flex_1().child(element))
                        .child(right),
                );
            } else {
                grid = grid.child(element);
            }
        }
    }
    grid
}

fn sub_field_is_half(field: &SubFormField) -> bool {
    match field {
        SubFormField::Select { .. } => true,
        SubFormField::Input {
            key,
            integer,
            browse,
            datetime,
            ..
        } => !*browse && !*datetime && (integer.is_some() || is_var_key(key)),
        _ => false,
    }
}
