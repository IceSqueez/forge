use super::editor::{field_wrap, step_glyph, sub_category_color, sub_field_is_half};
use super::*;
use forge_components::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_SM, FONT_XS, FONT_XXS, Radius,
    Spacing, anchored_popover, ghost_button_with_icon, modal, primary_button, radius,
    secondary_button, spacing, toggle,
};
use gpui::{FontWeight, Rgba};

impl ScreenActionsView {
    #[allow(clippy::too_many_arguments)]
    fn render_select_field(
        &self,
        key: &str,
        label: &str,
        options: &[(String, String)],
        selected: &str,
        open_picker: Option<&SelectPickerForm>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = options
            .iter()
            .find(|(value, _)| value == selected)
            .map(|(_, label)| label.clone());
        let (display, display_color): (String, Rgba) = match selected_label {
            Some(label) => (label, palette.text_primary),
            None if !selected.is_empty() => (selected.to_owned(), palette.text_primary),
            None => (tr!("actions_sub_select_placeholder"), palette.text_faint),
        };

        let key_open = key.to_owned();
        let border_color = if open_picker.is_some() {
            palette.brand
        } else {
            palette.border_input
        };
        let hover_border = palette.brand;
        let trigger = div()
            .id(SharedString::from(format!("actions-sub-select-{key}")))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(px(6.0))
            .px(px(10.0))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.shell)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                this.open_select_picker(key_open.clone(), ev.position(), window, cx)
            }))
            .child(
                div()
                    .flex_1()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(display_color)
                    .child(display),
            )
            .child(icon(Icon::ChevronDown, FONT_SM, palette.text_faint));

        let popover = open_picker.map(|form| {
            let view = cx.entity();
            anchored_popover(form.pos, form.picker.clone())
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_select_picker(cx));
                })
                .into_any_element()
        });

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(label.to_owned()),
            )
            .child(trigger)
            .children(popover)
            .into_any_element()
    }

    pub(super) fn render_sub_action_modal(
        &self,
        form: &EditSubActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let runner = self.sub_action_registry.get(&form.kind_id);
        let (header_glyph, header_color) = step_glyph(
            &form.kind_id,
            runner.map(|r| r.icon_name()).unwrap_or("layout-grid"),
            runner.map(|r| sub_category_color(r.category(), palette)),
            palette,
        );
        let chain_len = self.current_chain().len();
        let (step_index, step_total) = match form.target {
            SubFormTarget::Edit(i) => (i + 1, chain_len),
            SubFormTarget::Add => (chain_len + 1, chain_len + 1),
        };
        let bool_vals: HashMap<&str, bool> = form
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

        let mut grid_items: Vec<(bool, AnyElement)> = Vec::new();
        for field in &form.fields {
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
                    let control: AnyElement = if *browse {
                        let target_input = input.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, Density::Cozy))
                            .child(div().flex_1().child(input.clone()))
                            .child(
                                ghost_button_with_icon(
                                    Icon::Folder,
                                    tr!("actions_sub_file_browse"),
                                    palette,
                                )
                                .on_click(
                                    SharedString::from(format!("actions-sub-browse-{key}")),
                                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.browse_sub_field(target_input.clone(), cx)
                                    }),
                                ),
                            )
                            .into_any_element()
                    } else if *datetime {
                        let target_input = input.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, Density::Cozy))
                            .child(div().flex_1().child(input.clone()))
                            .child(
                                ghost_button_with_icon(
                                    Icon::Calendar,
                                    tr!("actions_sub_datetime_pick"),
                                    palette,
                                )
                                .on_click(
                                    SharedString::from(format!("actions-sub-datetime-{key}")),
                                    cx.listener(move |this, ev: &ClickEvent, _, cx| {
                                        this.open_datetime_picker(
                                            target_input.clone(),
                                            ev.position(),
                                            cx,
                                        )
                                    }),
                                ),
                            )
                            .into_any_element()
                    } else {
                        input.clone().into_any_element()
                    };
                    grid_items.push((
                        sub_field_is_half(field),
                        field_wrap(label, control, palette),
                    ));
                }
                SubFormField::Area {
                    label,
                    gate,
                    syntax,
                    area,
                    ..
                } => {
                    if !gate_on(gate) {
                        continue;
                    }
                    let lang_tag = syntax.map(|lang| match lang {
                        CodeLanguage::Rhai => "rhai",
                        CodeLanguage::Json => "json",
                    });
                    let mut header = div()
                        .flex()
                        .items_center()
                        .gap(spacing(Spacing::Xs, Density::Cozy))
                        .child(
                            div()
                                .font_family(DEFAULT_MONO_FAMILY)
                                .text_size(FONT_XXS)
                                .text_color(palette.text_muted)
                                .child(label.clone()),
                        );
                    if let Some(tag) = lang_tag {
                        header = header.child(
                            div()
                                .font_family(DEFAULT_MONO_FAMILY)
                                .text_size(FONT_XXS)
                                .text_color(palette.text_muted)
                                .child(tag),
                        );
                    }
                    grid_items.push((
                        false,
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(header)
                            .child(area.clone())
                            .into_any_element(),
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
                    let toggle_key = key.clone();
                    grid_items.push((
                        false,
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(spacing(Spacing::Sm, Density::Cozy))
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(label.clone()),
                            )
                            .child(toggle(*value, palette).on_click(
                                SharedString::from(format!("actions-sub-toggle-{key}")),
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.toggle_sub_field(toggle_key.clone(), cx)
                                }),
                            ))
                            .into_any_element(),
                    ));
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
                    let open_picker = form
                        .select_picker
                        .as_ref()
                        .filter(|picker_form| picker_form.key == *key);
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
                SubFormField::Hint { label } => {
                    grid_items.push((
                        false,
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(
                                div()
                                    .font_family(DEFAULT_MONO_FAMILY)
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_muted)
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_faint)
                                    .child(tr!("action_editor_branch_modal_hint")),
                            )
                            .into_any_element(),
                    ));
                }
            }
        }

        let mut grid = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy));
        if grid_items.is_empty() {
            grid = grid.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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

        let continue_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, Density::Cozy))
                    .child(icon(Icon::AlertTriangle, CARD_GLYPH, palette.warning))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, Density::Cozy))
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(tr!("actions_step_continue_on_error")),
                            )
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_faint)
                                    .child(tr!("actions_step_continue_on_error_hint")),
                            ),
                    ),
            )
            .child(toggle(form.continue_on_error, palette).on_click(
                "actions-sub-continue-on-error",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_sub_continue_on_error(cx)),
            ));

        let condition_field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_step_condition_label")),
            )
            .child(form.condition_input.clone())
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("actions_step_condition_hint")),
            );

        let advanced = div()
            .w_full()
            .mt(spacing(Spacing::Xs, Density::Cozy))
            .pt(spacing(Spacing::Sm, Density::Cozy))
            .border_t(HALF_BORDER)
            .border_color(palette.border_regular)
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_step_advanced")),
            )
            .child(condition_field)
            .child(continue_row);

        let body = div()
            .id("actions-sub-scroll")
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .py(spacing(Spacing::Sm, Density::Cozy))
            .max_h(SUB_MODAL_MAX_H)
            .overflow_y_scroll()
            .child(grid)
            .child(advanced);

        let title_slot = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .overflow_hidden()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(form.name_input.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(
                        "actions_step_subtitle",
                        index = step_index as i64,
                        total = step_total as i64
                    )),
            );

        let cancel = secondary_button(tr!("common_cancel"), palette).on_click(
            "actions-sub-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
        );
        let save = primary_button(tr!("actions_modal_save_btn"), palette).on_click(
            "actions-sub-submit",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit_sub_action(cx)),
        );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let card = modal("", body, palette)
            .width(STEP_MODAL_W)
            .header_icon(header_glyph, header_color)
            .header_tile_size(STEP_TILE, STEP_TILE_GLYPH)
            .title_slot(title_slot)
            .flush_body()
            .footer(footer)
            .on_close(
                "actions-sub-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-sub-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_sub_action(cx));
            })
            .into_any_element()
    }
}
