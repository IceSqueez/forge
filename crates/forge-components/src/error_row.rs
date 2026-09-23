use gpui::{IntoElement, ParentElement, Pixels, SharedString, Styled, div, px};

use crate::icons::{Icon, icon};
use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{BORDER_THIN, Density, FONT_SM, Radius, Spacing, body_family, radius, spacing};

const GLYPH: Pixels = px(13.0);
const PAD_V: Pixels = px(6.0);
const WASH: f32 = 0.1;

pub fn error_row(message: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
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
                .child(message.into()),
        )
}
