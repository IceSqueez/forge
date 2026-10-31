use super::rejection::rejection_note;
use super::*;
use crate::actions_screen::editor::{StepGroup, step_glyph};
use crate::actions_screen::files_root_note::files_root_note;
use forge_components::BORDER_THIN;
use forge_components::OverflowRevealingFocus;
use forge_components::{
    FONT_XS, FONT_XXS, Spacing, body_family, modal, mono_family, primary_button, scaled,
    secondary_button, spacing, toggle,
};
use gpui::FontWeight;

const TITLE_GAP: Pixels = px(2.0);

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
        let density = cx.density();
        let (header_glyph, header_color) = step_glyph(
            &self.kind_id,
            &self.icon_name,
            self.group.as_ref().map(|group| group.color(palette)),
            palette,
        );
        let (grid, rejection_shown_inline) = self.render_field_grid(palette, cx);

        let advanced = self.render_advanced(palette, cx);

        let body = div()
            .id("actions-sub-scroll")
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Sm, density))
            .max_h(SUB_MODAL_MAX_H)
            .min_h_0()
            .overflow_y_revealing_focus()
            .when(
                self.group.as_ref().is_some_and(StepGroup::is_files),
                |body| body.child(files_root_note(palette, density)),
            )
            .child(grid)
            .child(advanced);

        let title_slot = self.render_title_slot(palette, density);

        let footer = self.render_footer(rejection_shown_inline, palette, cx);

        let card = modal("", body, palette)
            .width(STEP_MODAL_W)
            .header_icon(header_glyph, header_color)
            .header_tile_size(STEP_TILE, STEP_TILE_GLYPH)
            .title_slot(title_slot)
            .flush_body()
            .body_scrolls_itself()
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

    fn render_continue_row(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(icon(Icon::AlertTriangle, CARD_GLYPH, palette.warning))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Xxs, density))
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
            ))
    }

    fn render_condition_field(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
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
            )
    }

    fn render_advanced(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        div()
            .w_full()
            .mt(spacing(Spacing::Xs, density))
            .pt(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.border_regular)
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("actions_step_advanced")),
            )
            .child(self.render_condition_field(palette, density))
            .child(self.render_continue_row(palette, cx))
    }

    fn render_title_slot(
        &self,
        palette: &ForgePalette,
        density: Density,
    ) -> impl IntoElement + use<> {
        let (step_index, step_total) = match self.target {
            SubFormTarget::Edit(i) => (i + 1, self.chain_len),
            SubFormTarget::Add => (self.chain_len + 1, self.chain_len + 1),
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(scaled(TITLE_GAP, density))
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
                        index = i64::try_from(step_index).unwrap_or(i64::MAX),
                        total = i64::try_from(step_total).unwrap_or(i64::MAX)
                    )),
            )
    }

    fn render_footer(
        &self,
        rejection_shown_inline: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
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
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().min_w_0().children(footer_rejection))
            .child(cancel)
            .child(save)
    }
}
