use super::field::{InputFlavor, SubFormField, field_gate, field_keys};
use super::field_controls::{bool_row, field_wrap, hint_block, multiline_field};
use super::rejection::with_rejection_note;
use super::variable_insert::InsertTarget;
use super::*;
use crate::config_field_label::{sender_bot_warning, sender_field_helper};
use crate::option_choices::ChoiceOption;
use forge_components::{ActivePresentation, FONT_SM, FONT_XXS, Spacing, scaled, spacing};
use forge_registry::CodeLanguage;
use gpui::{App, Div};

impl EditSubActionForm {
    pub(super) fn render_field_grid(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> (Div, bool) {
        let density = cx.density();
        let bool_vals: HashMap<&str, bool> = self
            .fields
            .iter()
            .filter_map(|f| match f {
                SubFormField::Bool { key, value, .. } => Some((key.as_str(), *value)),
                _ => None,
            })
            .collect();
        let gate_on = |gate: Option<&String>| {
            gate.map(|g| bool_vals.get(g.as_str()).copied().unwrap_or(false))
                .unwrap_or(true)
        };

        let rejected_key = self
            .rejection
            .as_ref()
            .and_then(|rejection| rejection.field_key.as_deref());
        let mut rejection_shown_inline = false;
        let mut grid_items: Vec<(bool, AnyElement)> = Vec::new();
        for field in &self.fields {
            if !gate_on(field_gate(field)) {
                continue;
            }
            let (half, mut element) = self.field_item(field, palette, cx);
            let names_rejected_key =
                rejected_key.is_some_and(|rejected| field_keys(field).contains(&rejected));
            if names_rejected_key
                && let Some(message) = self.rejection.as_ref().map(|r| r.message.clone())
            {
                rejection_shown_inline = true;
                element = with_rejection_note(element, message, palette, density, cx);
            }
            grid_items.push((half, element));
        }

        (
            field_grid_layout(grid_items, palette, density, cx),
            rejection_shown_inline,
        )
    }

    fn field_item(
        &self,
        field: &SubFormField,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> (bool, AnyElement) {
        let density = cx.density();
        match field {
            SubFormField::Input { label, .. } => {
                let control = self.input_control(field, palette, cx);
                (
                    sub_field_is_half(field),
                    field_wrap(label, control, palette),
                )
            }
            SubFormField::Area {
                key, label, area, ..
            } => {
                let button =
                    self.variable_button(key, InsertTarget::Area(area.clone()), palette, cx);
                let block = multiline_field(
                    label,
                    None,
                    Some(button),
                    area.clone().into_any_element(),
                    palette,
                    density,
                    cx,
                );
                (false, self.with_variable_popover(key, block, cx))
            }
            SubFormField::Code {
                key,
                label,
                language,
                editor,
                ..
            } => {
                let tag = match language {
                    CodeLanguage::Rhai => "rhai",
                    CodeLanguage::Json => "json",
                };
                let button = field.accepts_variables().then(|| {
                    self.variable_button(key, InsertTarget::Code(editor.clone()), palette, cx)
                });
                let block = multiline_field(
                    label,
                    Some(tag),
                    button,
                    editor.clone().into_any_element(),
                    palette,
                    density,
                    cx,
                );
                (false, self.with_variable_popover(key, block, cx))
            }
            SubFormField::Bool {
                key, label, value, ..
            } => (false, bool_row(key, label, *value, palette, cx)),
            SubFormField::Select {
                key,
                label,
                options,
                selected,
                ..
            } => (
                true,
                self.select_item(key, label, options, selected, palette, cx),
            ),
            SubFormField::UnitAmount {
                label,
                unit_picker_key,
                scale,
                unit,
                input,
                ..
            } => {
                let control =
                    self.unit_amount_control(unit_picker_key, scale, unit, input, palette, cx);
                (false, field_wrap(label, control, palette))
            }
            SubFormField::Hint { label } => (false, hint_block(label, palette, density, cx)),
        }
    }

    fn select_item(
        &self,
        key: &str,
        label: &str,
        options: &[ChoiceOption],
        selected: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let open_picker = self.open_picker_for(key);
        let select =
            self.render_select_field(key, label, options, selected, open_picker, palette, cx);
        let select = match sender_field_helper(&self.kind_id, key) {
            Some(helper) => with_field_helper(select, helper, palette, density, cx),
            None => select,
        };
        match sender_bot_warning(&self.kind_id, key, selected) {
            Some(warning) => with_field_warning(select, warning, palette, density, cx),
            None => select,
        }
    }
}

fn field_grid_layout(
    grid_items: Vec<(bool, AnyElement)>,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> Div {
    let mut grid = div().flex().flex_col().gap(spacing(Spacing::Sm, density));
    if grid_items.is_empty() {
        grid = grid.child(
            div()
                .font_family(cx.body_family())
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
                        .gap(scaled(GRID_COL_GAP, density))
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
        SubFormField::Input { flavor, .. } => {
            matches!(
                flavor,
                InputFlavor::Integer(_) | InputFlavor::OneBased(_) | InputFlavor::VariableName
            )
        }
        _ => false,
    }
}

fn with_field_warning(
    element: AnyElement,
    warning: String,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(element)
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .text_color(palette.warning)
                .child(warning),
        )
        .into_any_element()
}

fn with_field_helper(
    element: AnyElement,
    helper: String,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(element)
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(helper),
        )
        .into_any_element()
}
