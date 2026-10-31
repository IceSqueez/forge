use super::TtsFiltersView;
use forge_components::{
    BORDER_THIN, DISABLED_OPACITY, Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    KeyActivated, Radius, Spacing, TINT_FILL_ALPHA, body_family, card, error_row_with_retry, icon,
    mono_family, radius, scaled, spacing, toggle, tr, with_alpha,
};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Rgba, SharedString, Window, div, px};

const STAGE_CIRCLE: Pixels = px(22.0);
const PIPELINE_PAD_Y: Pixels = px(18.0);
const PIPELINE_PAD_X: Pixels = px(22.0);
const PIPELINE_GAP: Pixels = px(14.0);

impl TtsFiltersView {
    pub(super) fn pipeline_column(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let intro = div()
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(tr!("tts_filters_pipeline_intro"));

        let mut col = div()
            .flex()
            .flex_col()
            .gap(scaled(PIPELINE_GAP, density))
            .py(scaled(PIPELINE_PAD_Y, density))
            .px(scaled(PIPELINE_PAD_X, density))
            .child(intro);

        if let Some(err) = &self.save_error {
            col = col.child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .rounded(radius(Radius::Sm))
                    .border(BORDER_THIN)
                    .border_color(palette.danger)
                    .bg(with_alpha(palette.danger, TINT_FILL_ALPHA))
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.danger)
                    .child(err.clone()),
            );
        }

        if let Some(reason) = self.filters.failure() {
            col = col.child(error_row_with_retry(
                "filt-load-retry",
                tr!("tts_filters_load_failed", reason = reason),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_load(cx)),
                density,
            ));
        }

        let mut stages = div()
            .flex()
            .flex_col()
            .gap(scaled(PIPELINE_GAP, density))
            .child(self.skip_card(palette, density, cx))
            .child(self.blocklist_card(palette, density, cx))
            .child(self.replacements_card(palette, density, cx))
            .child(self.output_card(palette, density, cx));
        if !self.filters.is_loaded() {
            stages = stages.opacity(DISABLED_OPACITY);
        }
        col = col.child(stages);

        div()
            .id("filt-pipeline")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_y_scroll()
            .child(col)
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    pub(super) fn stage_frame(
        &self,
        n: u8,
        glyph: Icon,
        color: Rgba,
        title: impl Into<SharedString>,
        add: Option<AnyElement>,
        body: AnyElement,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let circle = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(STAGE_CIRCLE)
            .rounded(radius(Radius::Pill))
            .bg(color)
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.shell)
                    .child(n.to_string()),
            );

        let mut header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(circle)
            .child(icon(glyph, FONT_SM, color))
            .child(
                div()
                    .flex_1()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(title.into()),
            );
        if let Some(add) = add {
            header = header.child(add);
        }

        let body_wrap = div()
            .px(spacing(Spacing::Md, density))
            .py(spacing(Spacing::Xs, density))
            .child(body);

        card(
            div().flex().flex_col().child(header).child(body_wrap),
            palette,
        )
        .padding(px(0.0))
        .radius(Radius::Md)
        .full_width()
        .into_any_element()
    }

    pub(super) fn add_button(
        &self,
        id: &'static str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(handler)
            .child(icon(Icon::Plus, FONT_XS, palette.brand))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child(tr!("tts_filters_stage_add")),
            )
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    pub(super) fn stage_row(
        &self,
        label: SharedString,
        on: bool,
        meta: Option<SharedString>,
        toggle_id: SharedString,
        x_id: SharedString,
        divider: bool,
        deletable: bool,
        on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_x: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let label_color = if on {
            palette.text_primary
        } else {
            palette.text_muted
        };
        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Xs, density));
        if divider {
            row = row
                .border_b(BORDER_THIN)
                .border_color(palette.border_regular);
        }
        row = row
            .child(toggle(on, palette).on_click(toggle_id, on_toggle))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(label_color)
                    .child(label),
            );
        if let Some(meta) = meta {
            row = row.child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(meta),
            );
        }
        if deletable {
            row = row.child(
                div()
                    .id(x_id)
                    .flex()
                    .cursor_pointer()
                    .key_activated(palette.text_primary)
                    .on_click(on_x)
                    .child(icon(Icon::X, FONT_XS, palette.text_faint)),
            );
        }
        row.into_any_element()
    }
}
