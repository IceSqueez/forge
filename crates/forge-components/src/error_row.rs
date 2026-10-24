use gpui::{
    App, ClickEvent, Div, ElementId, IntoElement, ParentElement, Pixels, SharedString, Styled,
    Window, div, px,
};

use crate::buttons::ghost_button;
use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{
    BORDER_THIN, Density, FONT_SM, FONT_XS, Radius, Spacing, body_family, radius, spacing,
};
use crate::tr;

const GLYPH: Pixels = px(13.0);
const PAD_V: Pixels = px(6.0);
const WASH: f32 = 0.1;

fn error_frame(message: SharedString, palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .px(spacing(Spacing::Sm, Density::Cozy))
        .py(PAD_V)
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.random, WASH))
        .border(BORDER_THIN)
        .border_color(palette.random)
        .child(icon(Icon::AlertTriangle, GLYPH, palette.random))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.random)
                .child(message),
        )
}

pub fn error_row(message: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    error_frame(message.into(), palette)
}

pub fn compact_error_with_retry(
    id: impl Into<ElementId>,
    message: impl Into<SharedString>,
    palette: &ForgePalette,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let message: SharedString = message.into();
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_start()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .py(PAD_V)
        .rounded(radius(Radius::Sm))
        .bg(with_alpha(palette.random, WASH))
        .border(BORDER_THIN)
        .border_color(palette.random)
        .child(
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .child(icon(Icon::AlertTriangle, GLYPH, palette.random))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.random)
                        .child(message),
                ),
        )
        .child(ghost_button(tr!("widget_field_retry"), palette).on_click(id, on_retry))
}

pub fn error_row_with_retry(
    id: impl Into<ElementId>,
    message: impl Into<SharedString>,
    palette: &ForgePalette,
    on_retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    error_frame(message.into(), palette)
        .child(ghost_button(tr!("widget_field_retry"), palette).on_click(id, on_retry))
}
