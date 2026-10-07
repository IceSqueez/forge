use super::rejection::rejection_note;
use super::*;
use crate::actions_screen::editor::{step_glyph, sub_category_color};
use crate::actions_screen::files_root_note::files_root_note;
use forge_components::{
    FONT_XS, FONT_XXS, Spacing, body_family, modal, mono_family, primary_button, secondary_button,
    spacing, toggle,
};
use forge_registry::SubActionCategory;
use gpui::FontWeight;

impl EditSubActionForm {
    fn toggle_sub_continue_on_error(&mut self, cx: &mut Context<Self>) {
        self.continue_on_error = !self.continue_on_error;
        cx.notify();
    }

    pub(super) fn render_modal(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (header_glyph, header_color) = step_glyph(
            &self.kind_id,
            &self.icon_name,
            self.category.map(|c| sub_category_color(c, palette)),
            palette,
        );
        let (step_index, step_total) = match self.target {
            SubFormTarget::Edit(i) => (i + 1, self.chain_len),
            SubFormTarget::Add => (self.chain_len + 1, self.chain_len + 1),
        };
        let (grid, rejection_shown_inline) = self.render_field_grid(palette, cx);

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
                                    .font_family(body_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(tr!("actions_step_continue_on_error")),
                            )
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XXS)
                                    .text_color(palette.text_faint)
                                    .child(tr!("actions_step_continue_on_error_hint")),
                            ),
                    ),
            )
            .child(toggle(self.continue_on_error, palette).on_click(
                "actions-sub-continue-on-error",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_sub_continue_on_error(cx)),
            ));

        let condition_field = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_step_condition_label")),
            )
            .child(self.condition_input.clone())
            .child(
                div()
                    .font_family(body_family())
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
                    .font_family(mono_family())
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
            .when(self.category == Some(SubActionCategory::Files), |body| {
                body.child(files_root_note(palette))
            })
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
                    .child(self.name_input.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
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
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(tr!("actions_modal_save_btn"), palette).on_click(
            "actions-sub-submit",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
        );
        let footer_rejection = self
            .rejection
            .as_ref()
            .filter(|_| !rejection_shown_inline)
            .map(|rejection| rejection_note(rejection.message.clone(), palette));
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(div().flex_1().min_w_0().children(footer_rejection))
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
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-sub-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
            .into_any_element()
    }
}
